//! Linux TUN interface manager for `tun-game`
//!
//! Provides zero-overhead injection and extraction of raw IPv4/IPv6 packets.
//! Includes Linux native support (/dev/net/tun) and cross-platform mock for CI/Windows.

use std::io;
use tokio::sync::mpsc;
use tracing::info;
#[cfg(target_os = "linux")]
use tracing::{warn, error};

#[allow(dead_code)]
pub struct TunDevice {
    name: String,
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
}

impl TunDevice {
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
    pub async fn send(&self, packet: Vec<u8>) -> Result<(), mpsc::error::SendError<Vec<u8>>> {
        self.tx.send(packet).await
    }

    pub fn tx_handle(&self) -> mpsc::Sender<Vec<u8>> {
        self.tx.clone()
    }

    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.rx.recv().await
    }
}

#[cfg(target_os = "linux")]
pub fn create_tun(name: &str, ip_cidr: &str) -> io::Result<TunDevice> {
    use std::os::unix::io::AsRawFd;
    use std::fs::OpenOptions;
    use std::process::Command;

    const IFF_TUN: i16 = 0x0001;
    const IFF_NO_PI: i16 = 0x1000;
    const TUNSETIFF: u64 = 0x400454ca; // Linux TUNSETIFF ioctl number

    #[repr(C)]
    struct IfReq {
        ifr_name: [u8; 16],
        ifr_flags: i16,
        _pad: [u8; 22],
    }

    info!("Opening /dev/net/tun for interface: {}", name);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/net/tun")?;

    let mut ifr = IfReq {
        ifr_name: [0u8; 16],
        ifr_flags: IFF_TUN | IFF_NO_PI,
        _pad: [0u8; 22],
    };

    let name_bytes = name.as_bytes();
    let len = name_bytes.len().min(15);
    ifr.ifr_name[..len].copy_from_slice(&name_bytes[..len]);

    let ret = unsafe {
        libc::ioctl(file.as_raw_fd(), TUNSETIFF as libc::c_ulong, &ifr)
    };

    if ret < 0 {
        return Err(io::Error::last_os_error());
    }

    // Configure interface IP and bring it UP
    info!("Configuring TUN interface {} with IP {}", name, ip_cidr);
    let status_addr = Command::new("ip")
        .args(["addr", "add", ip_cidr, "dev", name])
        .status();
    if let Err(e) = status_addr {
        warn!("Failed to add IP to TUN dev via ip command: {}", e);
    }

    let status_up = Command::new("ip")
        .args(["link", "set", "dev", name, "up"])
        .status();
    if let Err(e) = status_up {
        warn!("Failed to bring up TUN dev via ip command: {}", e);
    }

    let read_file = file.try_clone()?;
    let mut write_file = file;

    let (to_tun_tx, mut to_tun_rx) = mpsc::channel::<Vec<u8>>(4096);
    let (from_tun_tx, from_tun_rx) = mpsc::channel::<Vec<u8>>(4096);

    // Background thread for writing to TUN
    std::thread::spawn(move || {
        use std::io::Write;
        while let Some(packet) = to_tun_rx.blocking_recv() {
            if let Err(e) = write_file.write_all(&packet) {
                error!("Error writing packet to TUN: {}", e);
                break;
            }
        }
    });

    // Background thread for reading from TUN
    std::thread::spawn(move || {
        use std::io::Read;
        let mut reader = read_file;
        let mut buf = [0u8; 65535];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if from_tun_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    error!("Error reading from TUN: {}", e);
                    break;
                }
            }
        }
    });

    Ok(TunDevice {
        name: name.to_string(),
        tx: to_tun_tx,
        rx: from_tun_rx,
    })
}

#[cfg(not(target_os = "linux"))]
pub fn create_tun(name: &str, ip_cidr: &str) -> io::Result<TunDevice> {
    info!("[Mock TUN] Initializing virtual mock interface '{}' with CIDR '{}'", name, ip_cidr);
    let (to_tun_tx, _to_tun_rx) = mpsc::channel::<Vec<u8>>(4096);
    let (_from_tun_tx, from_tun_rx) = mpsc::channel::<Vec<u8>>(4096);

    Ok(TunDevice {
        name: name.to_string(),
        tx: to_tun_tx,
        rx: from_tun_rx,
    })
}
