//! 报表订阅投递（M4 片E）：邮件（lettre）+ webhook（reqwest POST）。
//!
//! 与 billing 月报邮件略有差异（动态报表 xlsx 附件 + 任意数据集），故各写各的不强行抽象
//! （三行重复优于过早抽象；真有第三处需求再上提到共享层）。dry-run / 无 SMTP 时由调用方短路。

use lettre::{
    message::{header::ContentType, Attachment, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use rise_core::{AppError, AppResult, SmtpConfig};

const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// xlsx 附件（文件名 + 字节）。
pub struct XlsxAttachment {
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// 发送一封 HTML 邮件（可带单个 xlsx 附件）。收件人为空 → BadRequest。
pub async fn send_email(
    smtp: &SmtpConfig,
    recipients: &[String],
    subject: &str,
    html: String,
    attachment: Option<XlsxAttachment>,
) -> AppResult<()> {
    if recipients.is_empty() {
        return Err(AppError::BadRequest("收件人列表为空".into()));
    }
    let from: Mailbox = smtp
        .from
        .parse()
        .map_err(|e| AppError::BadRequest(format!("发件人地址非法 {}: {e}", smtp.from)))?;
    let mut builder = Message::builder().from(from).subject(subject);
    for r in recipients {
        let mb: Mailbox = r
            .parse()
            .map_err(|e| AppError::BadRequest(format!("收件人地址非法 {r}: {e}")))?;
        builder = builder.to(mb);
    }

    let html_part = SinglePart::html(html);
    let email = match attachment {
        Some(att) => {
            let ct = ContentType::parse(XLSX_MIME)
                .map_err(|e| AppError::Internal(format!("content-type: {e}")))?;
            let part = Attachment::new(att.filename).body(att.bytes, ct);
            builder.multipart(MultiPart::mixed().singlepart(html_part).singlepart(part))
        }
        None => builder.singlepart(html_part),
    }
    .map_err(|e| AppError::Internal(format!("构建邮件失败: {e}")))?;

    let transport = build_transport(smtp)?;
    transport
        .send(email)
        .await
        .map_err(|e| AppError::Internal(format!("SMTP 发送失败: {e}")))?;
    Ok(())
}

/// 按端口选 TLS：465 隐式 TLS / 587 STARTTLS / 其他明文（仅测试）。
fn build_transport(smtp: &SmtpConfig) -> AppResult<AsyncSmtpTransport<Tokio1Executor>> {
    let builder = match smtp.port {
        465 => AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)
            .map_err(|e| AppError::Internal(format!("smtp relay: {e}")))?,
        587 => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)
            .map_err(|e| AppError::Internal(format!("smtp starttls: {e}")))?,
        _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host),
    }
    .port(smtp.port);
    let builder = match (&smtp.user, &smtp.password) {
        (Some(u), Some(p)) => builder.credentials(Credentials::new(u.clone(), p.clone())),
        _ => builder,
    };
    Ok(builder.build())
}

/// POST 一个 JSON 负载到 webhook URL。2xx 外状态码视为失败（便于 cron 记日志、下轮补投）。
pub async fn post_webhook(url: &str, payload: &serde_json::Value) -> AppResult<()> {
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(payload)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("webhook 发送失败: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "webhook 返回非 2xx: {}",
            resp.status()
        )));
    }
    Ok(())
}
