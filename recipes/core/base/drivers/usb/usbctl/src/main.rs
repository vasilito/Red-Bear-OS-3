use clap::{Arg, Command};
use xhcid_interface::{PortId, XhciClientHandle};

fn main() {
    common::init();
    let matches = Command::new("usbctl")
        .arg(
            Arg::new("SCHEME")
                .num_args(1)
                .required(true)
                .long("scheme")
                .short('s'),
        )
        .subcommand(
            Command::new("port")
                .arg(Arg::new("PORT").num_args(1).required(true))
                .subcommand(Command::new("status"))
                .subcommand(
                    Command::new("endpoint")
                        .arg(Arg::new("ENDPOINT_NUM").num_args(1).required(true))
                        .subcommand(Command::new("status")),
                ),
        )
        .get_matches();

    let scheme = matches.get_one::<String>("SCHEME").expect("no scheme");

    if let Some(port_scmd_matches) = matches.subcommand_matches("port") {
        let port = port_scmd_matches
            .get_one::<String>("PORT")
            .expect("invalid utf-8 for PORT argument")
            .parse::<PortId>()
            .expect("expected PORT ID");

        let handle = XhciClientHandle::new(scheme.to_owned(), port)
            .expect("Failed to open XhciClientHandle");

        if let Some(_status_scmd_matches) = port_scmd_matches.subcommand_matches("status") {
            let state = handle.port_state().expect("Failed to get port state");
            println!("{}", state.as_str());
        } else if let Some(endp_scmd_matches) = port_scmd_matches.subcommand_matches("endpoint") {
            let endp_num = endp_scmd_matches
                .get_one::<String>("ENDPOINT_NUM")
                .expect("no valid ENDPOINT_NUM")
                .parse::<u8>()
                .expect("expected ENDPOINT_NUM to be an 8-bit integer");
            let mut endp_handle = handle
                .open_endpoint(endp_num)
                .expect("Failed to open endpoint");
            let state = endp_handle.status().expect("Failed to get endpoint state");
            println!("{}", state.as_str());
        }
    }
}
