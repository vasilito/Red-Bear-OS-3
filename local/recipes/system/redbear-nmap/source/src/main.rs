use std::env;
use std::io::{self, Read};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::process;
use std::time::Duration;

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-nmap: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return Err(usage());
    };

    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!("{}", usage());
        return Ok(());
    }

    let config = ScanConfig::parse(&args)?;
    let mut targets = resolve_targets(&config.host)?;
    if targets.is_empty() {
        return Err(format!("no addresses resolved for {}", config.host));
    }

    targets.sort();
    targets.dedup();

    println!(
        "scan_target={} ports={}",
        config.host,
        format_ports(&config.ports)
    );

    for addr in targets {
        println!("host {addr}");
        for port in &config.ports {
            let socket_addr = SocketAddr::new(addr, *port);
            let status = scan_port(socket_addr, config.timeout, config.banner_bytes);
            print_status(*port, &status);
        }
    }

    Ok(())
}

fn usage() -> String {
    "Usage: redbear-nmap [--timeout-ms N] [--banner-bytes N] <host> <ports>\n\nBounded scope: TCP connect scanning with optional banner reads only.\nNot implemented: raw/SYN scans, UDP parity, OS detection, packet capture, NSE.\n\nPorts can be a comma-separated list and/or ranges, for example: 22,80,443,8000-8003".to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScanConfig {
    host: String,
    ports: Vec<u16>,
    timeout: Duration,
    banner_bytes: usize,
}

impl ScanConfig {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut timeout_ms = 1000_u64;
        let mut banner_bytes = 64_usize;
        let mut positionals = Vec::new();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--timeout-ms" => {
                    let value = args.get(i + 1).ok_or_else(usage)?;
                    timeout_ms = value
                        .parse::<u64>()
                        .map_err(|err| format!("invalid --timeout-ms value {value}: {err}"))?;
                    i += 2;
                }
                "--banner-bytes" => {
                    let value = args.get(i + 1).ok_or_else(usage)?;
                    banner_bytes = value
                        .parse::<usize>()
                        .map_err(|err| format!("invalid --banner-bytes value {value}: {err}"))?;
                    i += 2;
                }
                other => {
                    positionals.push(other.to_string());
                    i += 1;
                }
            }
        }

        if positionals.len() != 2 {
            return Err(usage());
        }

        Ok(Self {
            host: positionals[0].clone(),
            ports: parse_ports(&positionals[1])?,
            timeout: Duration::from_millis(timeout_ms),
            banner_bytes,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PortStatus {
    Open { banner: Option<String> },
    Closed,
    TimedOut,
    Error(String),
}

fn scan_port(addr: SocketAddr, timeout: Duration, banner_bytes: usize) -> PortStatus {
    match TcpStream::connect_timeout(&addr, timeout) {
        Ok(mut stream) => {
            let _ = stream.set_read_timeout(Some(timeout));
            let banner = read_banner(&mut stream, banner_bytes);
            PortStatus::Open { banner }
        }
        Err(err) => match err.kind() {
            io::ErrorKind::ConnectionRefused => PortStatus::Closed,
            io::ErrorKind::TimedOut => PortStatus::TimedOut,
            _ => PortStatus::Error(err.to_string()),
        },
    }
}

fn read_banner(stream: &mut TcpStream, banner_bytes: usize) -> Option<String> {
    if banner_bytes == 0 {
        return None;
    }

    let mut buf = vec![0_u8; banner_bytes];
    match stream.read(&mut buf) {
        Ok(0) => None,
        Ok(count) => {
            buf.truncate(count);
            let text = String::from_utf8_lossy(&buf).trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
            ) =>
        {
            None
        }
        Err(_) => None,
    }
}

fn print_status(port: u16, status: &PortStatus) {
    match status {
        PortStatus::Open {
            banner: Some(banner),
        } => {
            println!("  port={port} state=open banner={:?}", banner);
        }
        PortStatus::Open { banner: None } => {
            println!("  port={port} state=open");
        }
        PortStatus::Closed => println!("  port={port} state=closed"),
        PortStatus::TimedOut => println!("  port={port} state=timed_out"),
        PortStatus::Error(err) => println!("  port={port} state=error detail={:?}", err),
    }
}

fn resolve_targets(host: &str) -> Result<Vec<IpAddr>, String> {
    (host, 0)
        .to_socket_addrs()
        .map(|iter| iter.map(|addr| addr.ip()).collect())
        .map_err(|err| format!("failed to resolve {host}: {err}"))
}

fn parse_ports(spec: &str) -> Result<Vec<u16>, String> {
    let mut ports = Vec::new();

    for part in spec
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        if let Some((start, end)) = part.split_once('-') {
            let start = parse_port_value(start)?;
            let end = parse_port_value(end)?;
            if start > end {
                return Err(format!(
                    "invalid port range {part}: start is greater than end"
                ));
            }
            for port in start..=end {
                ports.push(port);
            }
        } else {
            ports.push(parse_port_value(part)?);
        }
    }

    if ports.is_empty() {
        return Err("no ports provided".to_string());
    }

    ports.sort_unstable();
    ports.dedup();
    Ok(ports)
}

fn parse_port_value(text: &str) -> Result<u16, String> {
    let port = text
        .parse::<u16>()
        .map_err(|err| format!("invalid port {text}: {err}"))?;
    if port == 0 {
        return Err("port 0 is not supported".to_string());
    }
    Ok(port)
}

fn format_ports(ports: &[u16]) -> String {
    ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_port_lists_and_ranges() {
        assert_eq!(parse_ports("22,80,443").unwrap(), vec![22, 80, 443]);
        assert_eq!(parse_ports("22,80-82,80").unwrap(), vec![22, 80, 81, 82]);
    }

    #[test]
    fn rejects_invalid_ranges() {
        assert!(parse_ports("90-80").is_err());
        assert!(parse_ports("0").is_err());
        assert!(parse_ports("abc").is_err());
    }

    #[test]
    fn parses_scan_config_with_flags() {
        let args = vec![
            "--timeout-ms".to_string(),
            "250".to_string(),
            "--banner-bytes".to_string(),
            "16".to_string(),
            "example.com".to_string(),
            "22,80-81".to_string(),
        ];

        let config = ScanConfig::parse(&args).unwrap();
        assert_eq!(config.host, "example.com");
        assert_eq!(config.ports, vec![22, 80, 81]);
        assert_eq!(config.timeout, Duration::from_millis(250));
        assert_eq!(config.banner_bytes, 16);
    }
}
