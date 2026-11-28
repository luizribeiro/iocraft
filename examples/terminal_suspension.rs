use iocraft::prelude::*;
use std::{path::PathBuf, process::Command};

#[derive(Default, Props)]
struct EditorProps {
    path: PathBuf,
}

#[component]
fn EditFile(mut hooks: Hooks, props: &EditorProps) -> impl Into<AnyElement<'static>> {
    let terminal = hooks.use_terminal_suspension();
    let mut should_exit = hooks.use_state(|| false);
    let mut status = hooks.use_state(|| String::from("Press e to open $EDITOR or q to quit"));
    let path = props.path.clone();

    hooks.use_terminal_events(move |event| {
        let TerminalEvent::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event
        else {
            return;
        };
        match code {
            KeyCode::Char('e') => {
                let path = path.clone();
                terminal.suspend_terminal(move || {
                    status.set(match open_editor(path) {
                        Ok(()) => "Editor finished. Press e to reopen or q to quit".into(),
                        Err(error) => error,
                    });
                });
            }
            KeyCode::Char('q') => should_exit.set(true),
            _ => {}
        }
    });

    if should_exit.get() {
        hooks.use_context_mut::<SystemContext>().exit();
    }

    element!(Text(content: status.to_string()))
}

fn open_editor(path: PathBuf) -> Result<(), String> {
    let editor = std::env::var_os("EDITOR").ok_or("Set EDITOR to the editor executable")?;
    let status = Command::new(editor)
        .arg(path)
        .status()
        .map_err(|error| format!("Could not launch editor: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("Editor failed with {status}"))
}

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .expect("usage: terminal_suspension FILE");
    smol::block_on(element!(EditFile(path: PathBuf::from(path))).fullscreen()).unwrap();
}
