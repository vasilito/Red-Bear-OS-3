use std::process::Command;

#[allow(dead_code)]
pub fn spawn_driver(command: &[String]) -> Result<std::process::Child, std::io::Error> {
    if command.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty command",
        ));
    }

    let mut cmd = Command::new(&command[0]);
    for arg in &command[1..] {
        cmd.arg(arg);
    }

    cmd.spawn()
}
