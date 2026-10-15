//! One SMTP dialogue to 127.0.0.1:25, else ::1.
//!
//! Prints `RCPT <code>` and, only after RCPT 250, `DATA <code>`.
//! Does not print the message body. Recipients must end in `@example.test`.

use std::io::{BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::Duration;

use crate::error::ToolError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpAcceptReport {
    pub lines: Vec<String>,
    pub success: bool,
    pub error: Option<String>,
}

pub fn smtp_accept_io<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    rcpt: &str,
) -> SmtpAcceptReport {
    if let Some(report) = reject_rcpt(rcpt) {
        return report;
    }
    dialogue(reader, writer, rcpt)
}

pub fn run_vm_lab_smtp_accept(rcpt: &str) -> SmtpAcceptReport {
    if let Some(report) = reject_rcpt(rcpt) {
        return report;
    }
    let stream = match connect_smtp() {
        Ok(stream) => stream,
        Err(err) => return fail_report(Vec::new(), err.to_string()),
    };
    if let Err(err) = stream.set_read_timeout(Some(Duration::from_secs(20))) {
        return fail_report(Vec::new(), err.to_string());
    }
    if let Err(err) = stream.set_write_timeout(Some(Duration::from_secs(20))) {
        return fail_report(Vec::new(), err.to_string());
    }
    let mut writer = match stream.try_clone() {
        Ok(cloned) => cloned,
        Err(err) => return fail_report(Vec::new(), err.to_string()),
    };
    let mut reader = std::io::BufReader::new(stream);
    smtp_accept_io(&mut reader, &mut writer, rcpt)
}

fn reject_rcpt(rcpt: &str) -> Option<SmtpAcceptReport> {
    if !rcpt.contains('@')
        || rcpt
            .bytes()
            .any(|b| matches!(b, b'\r' | b'\n' | b' ' | b'<' | b'>'))
    {
        return Some(fail_report(Vec::new(), "refusing recipient".to_string()));
    }
    if !rcpt.ends_with("@example.test") {
        return Some(fail_report(
            Vec::new(),
            "refusing non-example.test recipient".to_string(),
        ));
    }
    None
}

fn dialogue<R: BufRead, W: Write>(reader: &mut R, writer: &mut W, rcpt: &str) -> SmtpAcceptReport {
    let banner = match read_smtp_reply(reader) {
        Ok(code) => code,
        Err(err) => return fail_report(Vec::new(), err.to_string()),
    };
    if banner != 220 {
        return fail_report(Vec::new(), format!("banner {banner}"));
    }
    let ehlo = match send_cmd(writer, reader, "EHLO mail.example.test") {
        Ok(code) => code,
        Err(err) => return fail_report(Vec::new(), err.to_string()),
    };
    if ehlo != 250 {
        return fail_report(Vec::new(), "ehlo refused".to_string());
    }
    let mail = match send_cmd(writer, reader, "MAIL FROM:<>") {
        Ok(code) => code,
        Err(err) => return fail_report(Vec::new(), err.to_string()),
    };
    if mail != 250 {
        return fail_report(Vec::new(), format!("mail from {mail}"));
    }
    let rcpt_code = match send_cmd(writer, reader, &format!("RCPT TO:<{rcpt}>")) {
        Ok(code) => code,
        Err(err) => return fail_report(Vec::new(), err.to_string()),
    };
    let mut lines = vec![format!("RCPT {rcpt_code}")];
    if rcpt_code != 250 {
        let _ = send_cmd(writer, reader, "QUIT");
        return SmtpAcceptReport {
            lines,
            success: true,
            error: None,
        };
    }
    let greeting = match send_cmd(writer, reader, "DATA") {
        Ok(code) => code,
        Err(err) => return fail_report(lines, err.to_string()),
    };
    if greeting != 354 {
        return fail_report(lines, "data refused".to_string());
    }
    let blob = format!(
        "From: lab-sender@example.test\r\n\
To: {rcpt}\r\n\
Subject: vm-receive-proof\r\n\
Message-ID: <vm-receive-proof@example.test>\r\n\
\r\n\
vm-receive-proof\r\n\
.\r\n"
    );
    if let Err(err) = writer
        .write_all(blob.as_bytes())
        .and_then(|()| writer.flush())
    {
        return fail_report(lines, err.to_string());
    }
    let data_code = match read_smtp_reply(reader) {
        Ok(code) => code,
        Err(err) => return fail_report(lines, err.to_string()),
    };
    lines.push(format!("DATA {data_code}"));
    let _ = send_cmd(writer, reader, "QUIT");
    SmtpAcceptReport {
        lines,
        success: data_code == 250,
        error: None,
    }
}

fn fail_report(lines: Vec<String>, message: String) -> SmtpAcceptReport {
    SmtpAcceptReport {
        lines,
        success: false,
        error: Some(message),
    }
}

fn send_cmd<W: Write, R: BufRead>(
    writer: &mut W,
    reader: &mut R,
    command: &str,
) -> Result<u16, ToolError> {
    writer.write_all(command.as_bytes())?;
    writer.write_all(b"\r\n")?;
    writer.flush()?;
    read_smtp_reply(reader)
}

fn read_smtp_reply<R: BufRead>(reader: &mut R) -> Result<u16, ToolError> {
    let mut count = 0u32;
    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 {
            return Err(ToolError::fail("smtp closed".to_string()));
        }
        count += 1;
        if count > 40 {
            return Err(ToolError::fail("smtp reply too long".to_string()));
        }
        if line.len() >= 4 && line[3] == b' ' {
            let text = std::str::from_utf8(&line[..3])
                .map_err(|_| ToolError::fail("smtp reply had no code".to_string()))?;
            let code = text
                .parse::<u16>()
                .map_err(|_| ToolError::fail("smtp reply had no code".to_string()))?;
            return Ok(code);
        }
    }
}

fn connect_smtp() -> Result<TcpStream, ToolError> {
    let mut last = "smtp connect failed".to_string();
    for addr in [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 25),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 25),
    ] {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(10)) {
            Ok(stream) => return Ok(stream),
            Err(err) => last = err.to_string(),
        }
    }
    Err(ToolError::fail(format!("smtp connect failed: {last}")))
}
