//! The "Copy diagnostics" text: versions, hardware, permissions and recent log lines.

use std::ffi::CString;

use recast_input::{PermissionState, Permissions};

pub const LOG_LINES: usize = 200;

pub struct SystemInfo {
    pub app_version: String,
    pub macos: String,
    pub model: String,
    pub chip: String,
    pub permissions: Permissions,
}

fn sysctl(name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    let mut size = 0usize;
    // SAFETY: a null buffer asks for the value's size only.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size];
    // SAFETY: `buffer` holds `size` bytes, as sysctl reported.
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    buffer.truncate(size);
    let text = String::from_utf8_lossy(&buffer);
    Some(text.trim_end_matches('\0').trim().to_owned())
}

impl SystemInfo {
    pub fn current(app_version: &str) -> Self {
        let unknown = || "unknown".to_string();
        Self {
            app_version: app_version.to_owned(),
            macos: sysctl("kern.osproductversion").unwrap_or_else(unknown),
            model: sysctl("hw.model").unwrap_or_else(unknown),
            chip: sysctl("machdep.cpu.brand_string").unwrap_or_else(unknown),
            permissions: recast_input::check_permissions(),
        }
    }
}

fn state(state: PermissionState) -> &'static str {
    match state {
        PermissionState::Granted => "granted",
        PermissionState::Denied => "not granted",
        PermissionState::NotDetermined => "not asked",
    }
}

/// Hides the user's home folder and anything that looks like a credential.
pub fn redact(line: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    let mut out = if home.is_empty() {
        line.to_owned()
    } else {
        line.replace(&format!("{home}/"), "~/")
    };
    for key in ["token=", "key=", "password=", "secret=", "signature="] {
        let mut from = 0;
        while let Some(found) = out[from..].to_ascii_lowercase().find(key) {
            let mut start = from + found + key.len();
            let quote = out[start..]
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''));
            if let Some(q) = quote {
                start += q.len_utf8();
            }
            let end = out[start..]
                .find(|c: char| match quote {
                    Some(q) => c == q,
                    None => c.is_whitespace() || matches!(c, '&' | '"' | '\'' | ','),
                })
                .map_or(out.len(), |i| start + i);
            out.replace_range(start..end, "<redacted>");
            from = start + "<redacted>".len();
        }
    }
    out
}

pub fn text(info: &SystemInfo, log_lines: &[String], home: &str) -> String {
    let p = &info.permissions;
    let mut out = format!(
        "Recast {}\nmacOS {}\nModel {} ({})\n\nPermissions\n  Screen Recording: {}\n  Input Monitoring: {}\n  Microphone: {}\n\nLast {} log lines\n",
        info.app_version,
        info.macos,
        info.model,
        info.chip,
        state(p.screen_recording),
        state(p.input_monitoring),
        state(p.microphone),
        log_lines.len(),
    );
    for line in log_lines {
        out.push_str(&redact(line, home));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> SystemInfo {
        SystemInfo {
            app_version: "0.2.0".into(),
            macos: "27.0".into(),
            model: "Mac16,7".into(),
            chip: "Apple M4 Pro".into(),
            permissions: Permissions {
                screen_recording: PermissionState::Granted,
                input_monitoring: PermissionState::Denied,
                microphone: PermissionState::NotDetermined,
            },
        }
    }

    #[test]
    fn lists_versions_permissions_and_logs() {
        let lines = vec![
            "[INFO] started".to_string(),
            "[WARN] cannot open /Users/ada/Movies/Recast/Take.recast".to_string(),
        ];
        let text = text(&info(), &lines, "/Users/ada");
        assert!(text.starts_with("Recast 0.2.0\nmacOS 27.0\nModel Mac16,7 (Apple M4 Pro)\n"));
        assert!(text.contains("Screen Recording: granted"));
        assert!(text.contains("Input Monitoring: not granted"));
        assert!(text.contains("Microphone: not asked"));
        assert!(text.contains("Last 2 log lines\n[INFO] started\n"));
        assert!(text.contains("cannot open ~/Movies/Recast/Take.recast"));
        assert!(!text.contains("/Users/ada"));
    }

    #[test]
    fn credentials_never_reach_the_text() {
        let lines = vec![
            "GET ws://127.0.0.1:5123/?token=abc123def&w=2 failed".to_string(),
            "updater: Key=RWQabc signature=\"dW50cnVzdGVk\" Password=hunter2".to_string(),
        ];
        let text = text(&info(), &lines, "/Users/ada");
        for secret in ["abc123def", "RWQabc", "dW50cnVzdGVk", "hunter2"] {
            assert!(!text.contains(secret), "{secret} leaked:\n{text}");
        }
        assert!(text.contains("token=<redacted>&w=2"));
    }

    #[test]
    fn redact_leaves_plain_lines_alone() {
        assert_eq!(redact("nothing here", "/Users/ada"), "nothing here");
        assert_eq!(redact("/Users/adam/x", ""), "/Users/adam/x");
        assert_eq!(redact("/Users/adam/x", "/Users/ada"), "/Users/adam/x");
    }

    #[test]
    fn this_mac_reports_its_system() {
        let info = SystemInfo::current("0.0.0");
        assert!(info.macos.split('.').next().unwrap().parse::<u32>().is_ok());
        assert_ne!(info.model, "unknown");
    }
}
