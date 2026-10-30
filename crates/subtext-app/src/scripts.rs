//! The *Script* menu, and the scripts behind it.
//!
//! The folder is the user's, which puts scripts where snippets already are:
//! neither the menu bar nor the keymap can name an item the user has not
//! written yet. Both are therefore built at runtime — by [`crate::menus`],
//! which asks this module for the two pieces, [`items`] for the submenu's
//! contents and [`bindings`] for the shortcuts that go with them.
//!
//! # Running one
//!
//! A script is a process, and running one is not something the interface thread
//! may do: [`execute`] blocks until the child exits, so the caller hands it to
//! the background executor and comes back with an [`Outcome`]. The script's own
//! source decides what it is given and where its answer goes — see
//! [`subtext_core::scripts::Invocation`] — and the caller applies that answer,
//! because it needs a window.
//!
//! # Where this differs from CotEditor
//!
//! *Nothing is hidden behind the Option key.* Upstream runs a script on a plain
//! click, opens it in an editor on Option, and shows it in the Finder on
//! Option-Shift. GPUI's menu items carry no modifier state, so a click always
//! runs the script. Every other way to reach a script file — the folder itself,
//! *Open Scripts Folder* — is unchanged.
//!
//! *A failing script is reported.* Upstream only shows a script's standard
//! error, so a script that exits non-zero without writing one looks like it
//! worked. SubText says so instead; the status bar is where the rest of the
//! application admits what it cannot do.

use std::path::Path;
use std::process::{Command, Stdio};

use gpui_kit::{App, KeyBinding, Menu, MenuItem};

use subtext_core::scripts::{Item, Launch, Script};

use crate::actions;

/// The last item of the Script menu.
///
/// Upstream appends it after a separator whatever the folder holds, and it is
/// the one item in that menu that does not name a script.
pub(crate) const OPEN_SCRIPTS_FOLDER: &str = "Open Scripts Folder";

/// The items the Script menu holds for the user's scripts.
///
/// The folder's own entries first, then the way to the folder — which is
/// upstream's order and its shape. The folder item is there even when the folder
/// is empty or missing, because it is how a user gets a folder to put scripts
/// in at all.
pub(crate) fn items(items: &[Item]) -> Vec<MenuItem> {
    let mut menu = entries(items);
    if !menu.is_empty() {
        // A leading separator would divide nothing from something.
        menu.push(MenuItem::separator());
    }
    menu.push(MenuItem::action(
        OPEN_SCRIPTS_FOLDER,
        actions::script::OpenScriptsFolder,
    ));
    menu
}

fn entries(items: &[Item]) -> Vec<MenuItem> {
    items.iter().map(entry).collect()
}

fn entry(item: &Item) -> MenuItem {
    match item {
        Item::Script(script) => MenuItem::action(
            script.name.clone(),
            actions::script::RunScript {
                path: script.path.to_string_lossy().into_owned(),
            },
        ),
        Item::Folder { name, items } => MenuItem::submenu(Menu {
            name: name.clone().into(),
            items: entries(items),
            disabled: false,
        }),
        Item::Separator => MenuItem::separator(),
    }
}

/// The script bindings to add on top of `keymap`'s own.
///
/// The keys come from [`subtext_core::scripts::bindings`], which settles who
/// wins a contested one; what is decided here is whether the key is free at all.
/// A script must not take a key ⌘S or ⌘F already answers to, for the same reason
/// a snippet must not: the built-in commands are what the user expects those
/// keys to do, and the settings file can be edited by hand.
///
/// The keystrokes come from files on disk, so `KeyBinding::new` cannot take
/// them — it only accepts literals since gpui-pre 0.3.10. `KeyBinding::load` is
/// the dynamic equivalent and needs an `App` for the host OS.
pub(crate) fn bindings(
    keymap: &gpui_kit::Keymap,
    items: &[Item],
    scope: Option<&str>,
    cx: &App,
) -> Vec<KeyBinding> {
    subtext_core::scripts::bindings(items, scope)
        .into_iter()
        .filter(|(_, keystroke)| crate::snippets::free(keymap, keystroke))
        .filter_map(|(path, keystroke)| {
            KeyBinding::load(
                &keystroke,
                Box::new(actions::script::RunScript {
                    path: path.to_string_lossy().into_owned(),
                }),
                None,
                false,
                None,
                cx,
            )
            .ok()
        })
        .collect()
}

/// What running a script left behind.
pub(crate) struct Outcome {
    /// What the script wrote to its standard output.
    pub output: String,
    /// Why it did not finish, in the words the status bar should show.
    pub failure: Option<String>,
}

impl Outcome {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            failure: Some(message.into()),
        }
    }
}

/// Run `script` to completion, with `input` on its standard input and
/// `document` as its argument.
///
/// This blocks, so it belongs on the background executor. It is upstream's
/// `UnixScript.run()` with the window taken out: the checks it makes — the file
/// exists, it may be executed, it has something in it — happen here, and the
/// script's own tokens are read by the caller, which is where the document is.
pub(crate) fn execute(script: &Script, input: Option<&str>, document: Option<&Path>) -> Outcome {
    if !script.path.is_file() {
        return Outcome::failed(format!("The script “{}” is gone", script.name));
    }

    let launch = script.launch();
    // A file run directly has to be executable; one handed to a program does
    // not — `osascript` reads an AppleScript, and Windows has no execute bit at
    // all.
    if matches!(launch, Launch::File) && !is_executable(&script.path) {
        return Outcome::failed(format!(
            "The script “{}” is not executable — check the permissions of the file",
            script.name
        ));
    }

    let mut command = match launch {
        Launch::File => Command::new(&script.path),
        Launch::Program(program) => {
            let mut command = Command::new(program);
            command.arg(&script.path);
            command
        }
    };
    if let Some(document) = document {
        command.arg(document);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Outcome::failed(format!(
                "The script “{}” could not be started: {error}",
                script.name
            ));
        }
    };

    // The input is written and the pipe closed before the wait, or a script that
    // reads to the end of its standard input would wait for ever. A script that
    // never reads it closes the pipe underneath us, which is not a failure.
    if let Some(input) = input
        && let Some(mut stdin) = child.stdin.take()
    {
        use std::io::Write as _;
        let _ = stdin.write_all(input.as_bytes());
    }

    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(error) => {
            return Outcome::failed(format!(
                "The script “{}” could not be run: {error}",
                script.name
            ));
        }
    };

    let standard_error = String::from_utf8_lossy(&output.stderr);
    if !standard_error.trim().is_empty() {
        return Outcome::failed(standard_error.trim().to_string());
    }
    if !output.status.success() {
        return Outcome::failed(format!(
            "The script “{}” exited with {}",
            script.name,
            describe(output.status)
        ));
    }

    Outcome {
        output: String::from_utf8_lossy(&output.stdout).into_owned(),
        failure: None,
    }
}

/// How a script ended, in words a status bar can hold.
fn describe(status: std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("status {code}"),
        None => "a signal".to_string(),
    }
}

/// Whether the file may be run.
///
/// Windows has no execute bit — a file a user can read is one they can run — so
/// the question is only asked where the permission exists, which is why this is
/// two functions rather than one with a branch in it.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

/// The scripts a folder holds, as the menu bar needs them.
///
/// A free function rather than a method on the workspace so that the one thing
/// which has to be told where the folder is — [`crate::resources`] — is the only
/// thing that knows.
pub(crate) fn scan() -> Vec<Item> {
    subtext_core::scripts::scan(&crate::resources::script_directory())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use subtext_core::snippets::Shortcut;

    fn script(name: &str, file: &str, spec: Option<&str>) -> Script {
        Script {
            path: PathBuf::from("/scripts").join(file),
            name: name.to_string(),
            kind: subtext_core::ScriptKind::Unix,
            shortcut: spec.and_then(Shortcut::parse),
        }
    }

    /// The labels of some items, with separators marked and folders in
    /// parentheses so the shape of the menu is visible.
    fn labels(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => "―".to_string(),
                MenuItem::Action { name, .. } => name.to_string(),
                MenuItem::Submenu(submenu) => format!("({})", submenu.name),
                MenuItem::SystemMenu(_) => "?".to_string(),
            })
            .collect()
    }

    /// The folder's own entries, then a separator, then the way to the folder —
    /// which is upstream's shape.
    #[test]
    fn the_menu_ends_with_the_way_to_the_folder() {
        let folder = vec![
            Item::Script(script("Save", "Save.@s.sh", Some("@s"))),
            Item::Separator,
            Item::Folder {
                name: "Python".into(),
                items: vec![Item::Script(script("Clean", "Clean.py", None))],
            },
        ];
        assert_eq!(
            labels(&items(&folder)),
            ["Save", "―", "(Python)", "―", OPEN_SCRIPTS_FOLDER]
        );
    }

    /// An empty folder is still a way in: it is where a user puts the first
    /// script.
    #[test]
    fn an_empty_folder_still_has_its_way_in() {
        assert_eq!(labels(&items(&[])), [OPEN_SCRIPTS_FOLDER]);
    }

    /// The folder becomes a submenu, with the folder's own entries inside it.
    #[test]
    fn a_folder_becomes_a_submenu() {
        let folder = vec![Item::Folder {
            name: "Tools".into(),
            items: vec![
                Item::Script(script("One", "One.sh", None)),
                Item::Script(script("Two", "Two.sh", None)),
            ],
        }];
        let menu = items(&folder);
        let MenuItem::Submenu(submenu) = &menu[0] else {
            panic!("the folder is a submenu");
        };
        assert_eq!(submenu.name.as_ref(), "Tools");
        assert_eq!(labels(&submenu.items), ["One", "Two"]);
        assert!(!submenu.disabled, "a folder the user made is not disabled");
    }

    /// Every script item names its script by path, so an item that outlives a
    /// rearrangement still runs the script it was built for.
    #[test]
    fn a_script_item_names_the_script_it_stands_for() {
        let folder = vec![Item::Script(script("Save", "Save.@s.sh", Some("@s")))];
        let MenuItem::Action { action, .. } = &items(&folder)[0] else {
            panic!("a script is an action");
        };
        let run = action
            .as_any()
            .downcast_ref::<actions::script::RunScript>()
            .expect("a script item runs a script");
        assert_eq!(run.path, "/scripts/Save.@s.sh");
    }

    // ---- running one -----------------------------------------------------

    /// The part of this module that talks to the operating system, exercised
    /// for real: a `Command` built wrongly looks exactly like one built right
    /// until something runs it.
    ///
    /// Unix only, because the probe is a shell script and the execute bit is
    /// part of what it checks.
    #[cfg(unix)]
    mod running {
        use super::*;

        use std::fs;

        /// A script with `body` in it, executable or not, named after the test
        /// that made it.
        struct Probe(PathBuf);

        impl Probe {
            fn new(name: &str, body: &str, mode: u32) -> Self {
                let path = std::env::temp_dir()
                    .join(format!("subtext-probe-{}-{name}.sh", std::process::id()));
                fs::write(&path, body).expect("the probe is writable");
                let mut permissions = fs::metadata(&path).expect("the probe exists").permissions();
                std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, mode);
                fs::set_permissions(&path, permissions).expect("the probe is chmodable");
                Self(path)
            }

            fn script(&self) -> Script {
                Script {
                    path: self.0.clone(),
                    name: "Probe".into(),
                    kind: subtext_core::ScriptKind::Unix,
                    shortcut: None,
                }
            }

            fn run(&self, input: Option<&str>) -> Outcome {
                execute(&self.script(), input, Some(Path::new("/tmp/document.txt")))
            }
        }

        impl Drop for Probe {
            fn drop(&mut self) {
                fs::remove_file(&self.0).ok();
            }
        }

        /// The input reaches the script's standard input and what it writes back
        /// is caught.
        #[test]
        fn the_input_is_piped_in_and_the_output_is_caught() {
            let probe = Probe::new("pipe", "#!/bin/sh\ntr '[:lower:]' '[:upper:]'\n", 0o755);
            let outcome = probe.run(Some("hello"));
            assert_eq!(outcome.failure, None);
            assert_eq!(outcome.output, "HELLO");
        }

        /// A script that asks for no input is run with the pipe closed, so one
        /// that reads to the end of its input does not wait for ever.
        #[test]
        fn a_script_that_reads_nothing_is_run_with_the_pipe_closed() {
            let probe = Probe::new("no-input", "#!/bin/sh\ncat\necho done\n", 0o755);
            let outcome = probe.run(None);
            assert_eq!(outcome.failure, None);
            assert_eq!(outcome.output, "done\n");
        }

        /// A script's standard error is what the user is shown, which is
        /// upstream's rule.
        #[test]
        fn what_the_script_writes_to_standard_error_is_the_failure() {
            let probe = Probe::new(
                "stderr",
                "#!/bin/sh\necho 'something went wrong' >&2\nexit 1\n",
                0o755,
            );
            let outcome = probe.run(None);
            assert_eq!(outcome.failure.as_deref(), Some("something went wrong"));
            assert_eq!(outcome.output, "");
        }

        /// A script that fails silently is still reported — upstream only looks
        /// at standard error, so it would call this one a success.
        #[test]
        fn a_script_that_exits_non_zero_without_a_word_is_still_a_failure() {
            let probe = Probe::new("exit-code", "#!/bin/sh\nexit 3\n", 0o755);
            let outcome = probe.run(None);
            assert_eq!(
                outcome.failure.as_deref(),
                Some("The script “Probe” exited with status 3")
            );
        }

        /// A file that cannot be executed is reported before it is started,
        /// which is upstream's `ScriptFileError(.permission)`.
        #[test]
        fn a_script_that_is_not_executable_is_refused() {
            let probe = Probe::new("no-exec", "#!/bin/sh\necho hi\n", 0o644);
            let outcome = probe.run(None);
            assert!(
                outcome
                    .failure
                    .as_deref()
                    .is_some_and(|failure| failure.contains("not executable")),
                "{:?}",
                outcome.failure
            );
        }

        /// A script that went away between the menu being built and the item
        /// being chosen is reported as gone rather than run as something else.
        #[test]
        fn a_script_that_has_gone_is_reported_as_gone() {
            let probe = Probe::new("gone", "#!/bin/sh\n", 0o755);
            let script = probe.script();
            drop(probe);

            let outcome = execute(&script, None, None);
            assert_eq!(
                outcome.failure.as_deref(),
                Some("The script “Probe” is gone")
            );
        }
    }
}
