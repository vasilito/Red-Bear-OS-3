use std::path::PathBuf;

fn main() {
    let linux_kpi_headers = PathBuf::from("../../linux-kpi/source/src/c_headers");

    cc::Build::new()
        .file("src/linux_port.c")
        .include(linux_kpi_headers)
        .warnings(true)
        .compile("redbear_iwlwifi_linux_port");
}
