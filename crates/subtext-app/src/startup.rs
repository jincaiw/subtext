//! What the application does when it cannot start.
//!
//! A desktop launcher runs `subtext` with no terminal attached, so stdout and
//! stderr go nowhere a user can see. That makes every startup failure look the
//! same from the outside: the icon is clicked and nothing happens — sometimes
//! because the process died, and sometimes because GPUI went headless and is
//! sitting in an event loop with no window to draw into.
//!
//! This module gives a failed launch somewhere to speak:
//!
//! * a log file, truncated at every launch, that survives the process;
//! * a dialog from whatever the desktop provides (`zenity`, `kdialog`,
//!   `xmessage`, `notify-send`) when the window itself cannot open; and
//! * a non-zero exit status, so a wrapper script can tell the two apart.
//!
//! It is deliberately dependency-free: it runs before GPUI exists and has to
//! keep working when GPUI's platform or renderer cannot.
//!
//! Nothing here reports on its own. It is called from [`crate::run`], which
//! asks [`display_problem`] before starting the platform and [`fatal`] when a
//! window cannot be opened or a thread panics.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::localization::Language;

/// Set while the process is running as `subtext --diagnose`.
///
/// Diagnosis mode means a terminal is attached and the reader can copy the
/// error: the dialog would only cover that terminal, so it is skipped.
static DIAGNOSING: AtomicBool = AtomicBool::new(false);

/// Whether a panic has already been reported. A panic on a worker thread can
/// be followed by the main thread unwinding with the same message; one dialog
/// is enough, and a storm of them is worse than none.
static PANIC_REPORTED: AtomicBool = AtomicBool::new(false);

/// The log file of this launch, once the logger installed it.
static LOG_FILE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Install the logger and the panic hook.
///
/// Call this before `gpui_kit::application()`: constructing the platform and
/// connecting to the display server can panic, and those messages are exactly
/// what a desktop launch otherwise loses. `diagnose` turns the dialog off and
/// leaves the log on stdout, where a terminal can capture it.
pub fn install(diagnose: bool) {
    DIAGNOSING.store(diagnose, Ordering::Relaxed);

    let file = open_log_file();
    LOG_FILE.get_or_init(|| if file.is_some() { log_path() } else { None });

    let mut builder = env_logger::Builder::new();
    // `info` is the level GPUI reports the display server and the GPU adapter
    // list at, and those two lines are the diagnosis. `RUST_LOG` still wins.
    builder.filter_level(log::LevelFilter::Info);
    builder.parse_default_env();
    builder.format_timestamp_millis();
    builder.target(match file {
        Some(file) => env_logger::Target::Pipe(Box::new(Tee { file })),
        None => env_logger::Target::Stderr,
    });
    // A logger may already be installed when the library is driven from a
    // test; that is not a reason to abort the launch.
    let _ = builder.try_init();

    log::info!(
        "subtext {} starting (pid {})",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    );
    install_panic_hook();
}

/// The path of the log file this launch writes, if one could be created.
pub fn log_file() -> Option<&'static Path> {
    LOG_FILE.get().and_then(|file| file.as_deref())
}

/// The directory log files live in.
///
/// `$XDG_STATE_HOME` is what the XDG base directory specification reserves for
/// a program's own state, which is what a log is. The macOS and Windows
/// equivalents are their platform's log directories.
fn log_directory() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| home.join("Library").join("Logs").join("SubText"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().map(|directory| directory.join("SubText").join("Logs"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        state
            .or_else(|| dirs::home_dir().map(|home| home.join(".local").join("state")))
            .map(|directory| directory.join("subtext"))
    }
}

fn log_path() -> Option<PathBuf> {
    log_directory().map(|directory| directory.join("startup.log"))
}

fn open_log_file() -> Option<std::fs::File> {
    let path = log_path()?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).ok()?;
    }
    std::fs::File::create(path).ok()
}

/// A writer that puts every record in the log file *and* on stderr.
///
/// The terminal copy is not redundant: `cargo run` and `subtext --diagnose`
/// are read by a person, and the file is what survives a desktop launch.
struct Tee {
    file: std::fs::File,
}

impl std::io::Write for Tee {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buffer);
        self.file.write_all(buffer)?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        self.file.flush()
    }
}

/// A report of this launch, for `subtext --diagnose`.
///
/// It is printed before the platform starts. GPU details are not here because
/// enumerating adapters is GPUI's job; the log records the adapters it found,
/// and a failed launch points at the log.
pub fn report() -> String {
    use std::fmt::Write as _;

    let mut report = String::new();
    let _ = writeln!(report, "SubText {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        report,
        "platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(report, "system: {}", describe_system());
    let _ = writeln!(report, "glibc: {}", libc_version());
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "DESKTOP_SESSION",
    ] {
        let value = std::env::var(name).unwrap_or_else(|_| "(unset)".to_string());
        let _ = writeln!(report, "{name}={value}");
    }
    match crate::resources::Resources::discover() {
        Some(resources) => {
            let _ = writeln!(
                report,
                "resources: {} ({:?})",
                resources.root.display(),
                resources.origin
            );
        }
        None => {
            let _ = writeln!(
                report,
                "resources: not found — the editor opens without syntax definitions or themes"
            );
        }
    }
    match log_file() {
        Some(path) => {
            let _ = writeln!(report, "log: {}", path.display());
        }
        None => {
            let _ = writeln!(
                report,
                "log: could not be created; messages go to stderr only"
            );
        }
    }
    report
}

/// `PRETTY_NAME` from `os-release`, or the kernel release when it is missing.
fn describe_system() -> String {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        if let Ok(text) = std::fs::read_to_string("/etc/os-release")
            && let Some(name) = text
                .lines()
                .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        {
            return name.trim_matches('"').to_string();
        }
        if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
            return format!("Linux {}", release.trim());
        }
    }
    std::env::consts::OS.to_string()
}

fn libc_version() -> String {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: `gnu_get_libc_version` returns a pointer to a static,
        // NUL-terminated string that lives for the process's lifetime.
        let version = unsafe { libc::gnu_get_libc_version() };
        if version.is_null() {
            return "unknown".to_string();
        }
        // SAFETY: the pointer is non-null and NUL-terminated, as above.
        return unsafe { std::ffi::CStr::from_ptr(version) }
            .to_string_lossy()
            .into_owned();
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    "n/a".to_string()
}

/// Why this launch has no window to draw into, if the environment says so.
///
/// GPUI's Linux platform starts *headless* when neither `DISPLAY` nor
/// `WAYLAND_DISPLAY` is set: the process keeps running, opens no window, and
/// looks from the outside exactly like an application that does nothing.
/// Starting headless is right for a test harness and wrong for a desktop
/// launch, so this turns it into a reported failure. `ZED_HEADLESS` is GPUI's
/// own request for a headless run and is honoured.
pub fn display_problem() -> Option<String> {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        if std::env::var_os("ZED_HEADLESS").is_some() {
            return None;
        }
        if !names_display(
            std::env::var_os("DISPLAY"),
            std::env::var_os("WAYLAND_DISPLAY"),
        ) {
            return Some(if chinese() {
                "DISPLAY 和 WAYLAND_DISPLAY 都没有设置。\n\
                 桌面启动器没有把图形会话传进来，程序没有窗口可以画。\n\
                 请从桌面会话重新启动；如果在终端里运行，请确认这两个变量之一有值。"
                    .to_string()
            } else {
                "Neither DISPLAY nor WAYLAND_DISPLAY is set. The launcher did not pass a \
                 graphical session, so there is no surface to draw into. Start SubText from \
                 the desktop session; in a terminal, set one of the two variables."
                    .to_string()
            });
        }
    }
    None
}

/// Whether `environment` names a display server, given the two variables.
///
/// An empty value is the same as an unset one: `DISPLAY=` is what a broken
/// launcher produces, and it cannot be connected to.
#[cfg(any(target_os = "linux", target_os = "freebsd", test))]
fn names_display(display: Option<std::ffi::OsString>, wayland: Option<std::ffi::OsString>) -> bool {
    [display, wayland]
        .into_iter()
        .flatten()
        .any(|value| !value.is_empty())
}

/// Report a failure that leaves the application with nothing to show, then
/// exit non-zero.
///
/// The exit status is part of the interface: a launcher script can retry or
/// uninstall on it, and the log is already flushed because the writer does not
/// buffer.
pub fn fatal(summary: &str, detail: &str) -> ! {
    log::error!("{summary}: {detail}");
    eprintln!("subtext: {summary}: {detail}");
    if !DIAGNOSING.load(Ordering::Relaxed) {
        show_dialog(&summary_text(), &fatal_body(summary, detail));
    }
    std::process::exit(1);
}

/// The title of every fatal dialog and log summary.
pub fn summary_text() -> String {
    if chinese() {
        "SubText 无法启动".to_string()
    } else {
        "SubText could not start".to_string()
    }
}

/// The title of the alert a panic shows.
fn panic_title() -> String {
    if chinese() {
        "SubText 遇到错误".to_string()
    } else {
        "SubText ran into a problem".to_string()
    }
}

/// What the dialog says: what failed, the engine's reason, and where the log
/// is.
fn fatal_body(summary: &str, detail: &str) -> String {
    let mut body = format!("{summary}\n\n{detail}");
    if let Some(advice) = advice(detail) {
        body.push_str("\n\n");
        body.push_str(advice);
    }
    match log_file() {
        Some(path) if chinese() => {
            body.push_str(&format!("\n\n启动日志：{}", path.display()));
        }
        Some(path) => {
            body.push_str(&format!("\n\nStartup log: {}", path.display()));
        }
        None => {}
    }
    body
}

/// A suggestion for the two failures that have a known cause.
///
/// Kept as a pure function so the wording can be tested without a failing
/// GPU, and matched on the text GPUI and wgpu already produce.
fn advice(detail: &str) -> Option<&'static str> {
    let chinese = chinese();
    if detail.contains("No GPU adapter")
        || detail.contains("no compatible surface formats")
        || detail.contains("Failed to create wgpu device")
        // GPUI's other spelling of the same condition: the surface could not
        // be created on any enabled backend (seen in a container with no
        // EGL vendor and no Vulkan ICD).
        || detail.contains("Failed to create surface")
    {
        return Some(if chinese {
            "没有可用的图形设备，日志里列出了每个适配器被跳过的原因。\n\
             如果系统在虚拟机里，请打开虚拟机的 3D 加速；\n\
             否则请检查显卡驱动与 Mesa 是否完整（mesa-vulkan-drivers、libgl1-mesa-dri）。"
        } else {
            "No usable graphics device was found; the log lists why each adapter was skipped. \
             In a virtual machine, enable 3D acceleration; otherwise check that the graphics \
             driver and Mesa are complete (mesa-vulkan-drivers, libgl1-mesa-dri)."
        });
    }
    if detail.contains("failed to connect to X server")
        || detail.contains("failed to connect to Wayland")
        || detail.contains("failed to connect to display")
    {
        return Some(if chinese {
            "无法连接显示服务器。请确认程序是在桌面会话里启动的；\n\
             如果 WAYLAND_DISPLAY 指向一个已经退出的会话，可以先执行\n\
             env -u WAYLAND_DISPLAY subtext 再试。"
        } else {
            "The display server could not be reached. Start SubText from the desktop session; \
             if WAYLAND_DISPLAY points at a session that has ended, try \
             env -u WAYLAND_DISPLAY subtext."
        });
    }
    None
}

/// The language of the error text.
///
/// The application defaults to Simplified Chinese and says so in the settings
/// file; the startup failure can happen before that file is read, but reading
/// it here is what makes the dialog match the interface the user chose.
fn chinese() -> bool {
    matches!(
        crate::settings::Settings::load().language,
        Language::Chinese
    )
}

fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|message| message.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_string());
        let text = format!("panic at {location}: {message}");
        // The backtrace is taken here instead of trusting `RUST_BACKTRACE`:
        // the hook runs on the panicking thread's stack, and the file is the
        // only place a desktop launch can read it from.
        log::error!("{text}\n{}", std::backtrace::Backtrace::force_capture());
        default(info);

        if !DIAGNOSING.load(Ordering::Relaxed) && !PANIC_REPORTED.swap(true, Ordering::Relaxed) {
            // A panic is not always at startup — it can fire in a running
            // window — so the title says "problem", not "could not start".
            let title = panic_title();
            show_dialog(&title, &fatal_body(&title, &text));
        }
    }));
}

/// Show a dialog with whatever this desktop has, without waiting for it.
///
/// `zenity` is in a GNOME/Ubuntu install and on Kylin; `kdialog` covers KDE;
/// `xmessage` is the X fallback; `notify-send` at least puts the message in the
/// notification area when nothing can open a window. Spawning rather than
/// waiting matters: the process is about to exit, and a modal child would
/// leave the launcher with a live process after the editor is gone.
fn show_dialog(title: &str, body: &str) {
    // A script that runs the editor and wants the exit status, not a modal
    // window, sets this. It is also how the smoke test in a headless container
    // keeps `zenity` from waiting for a click that will never come.
    if std::env::var_os("SUBTEXT_NO_DIALOG").is_some() {
        return;
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let candidates: [(&str, Vec<&str>); 4] = [
            (
                "zenity",
                vec!["--error", "--no-wrap", "--title", title, "--text", body],
            ),
            ("kdialog", vec!["--title", title, "--error", body]),
            ("xmessage", vec!["-center", "-title", title, body]),
            ("notify-send", vec!["--app-name", "SubText", title, body]),
        ];
        for (program, arguments) in candidates {
            // `spawn` fails only when the program is missing, which is exactly
            // the test for "does this desktop have this dialog?".
            if std::process::Command::new(program)
                .args(arguments)
                .spawn()
                .is_ok()
            {
                return;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        // AppleScript's `display alert` is present on every macOS install, and
        // a bundle launched from the Finder has the same invisible stderr.
        let script = format!(
            "display alert {} message {} as critical",
            applescript_literal(title),
            applescript_literal(body)
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn();
    }
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
    {
        let _ = (title, body);
    }
}

/// An AppleScript string literal: [`crate::applescript::escape`] with the
/// surrounding quotes.
#[cfg(any(target_os = "macos", test))]
fn applescript_literal(text: &str) -> String {
    format!("\"{}\"", crate::applescript::escape(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_non_empty_display_variable_names_a_display() {
        use std::ffi::OsString;

        let os = |value: &str| Some(OsString::from(value));
        assert!(!names_display(None, None));
        // The empty value is what a launcher that clears the variable leaves
        // behind; it is not a display server.
        assert!(!names_display(os(""), os("")));
        assert!(names_display(os(""), os("wayland-0")));
        assert!(names_display(os(":0"), None));
    }

    #[test]
    fn gpu_failures_get_a_graphics_suggestion() {
        let hint = advice("No GPU adapter found that can configure the display surface")
            .expect("a GPU failure has advice");
        assert!(hint.contains("mesa-vulkan-drivers"));
        assert!(advice("Failed to create wgpu device: X").is_some());
        assert!(advice("no compatible surface formats").is_some());
        // The wording GPUI used in the container with no EGL vendor.
        assert!(
            advice(
                "could not open the main window: Failed to create surface: \
                 Failed to create surface for any enabled backend: {}"
            )
            .is_some()
        );
    }

    #[test]
    fn display_connection_failures_get_a_session_suggestion() {
        let hint = advice("failed to connect to X server \":0\": Unknown connection error")
            .expect("a display failure has advice");
        assert!(hint.contains("env -u WAYLAND_DISPLAY"));
    }

    #[test]
    fn an_unrecognized_failure_still_reports_itself() {
        assert!(advice("something else went wrong").is_none());
        let body = fatal_body(
            "The main window could not open",
            "something else went wrong",
        );
        assert!(body.contains("The main window could not open"));
        assert!(body.contains("something else went wrong"));
    }

    #[test]
    fn applescript_text_is_escaped_but_keeps_its_newlines() {
        let literal = applescript_literal("say \"hi\"\nC:\\tmp");
        assert_eq!(literal, "\"say \\\"hi\\\"\nC:\\\\tmp\"");
    }
}
