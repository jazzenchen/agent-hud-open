pub(crate) fn client_command_line(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| format!("'{}'", argument.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn configure_client_command(_command: &mut std::process::Command) {}

pub(crate) fn stop_client(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}
