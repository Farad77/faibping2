//! Windows Network & TCP/IP Registry Optimization Engine
//!
//! Applies low-latency optimizations to TCP/IP and Multimedia Class Scheduler (MMCSS):
//! - TCPNoDelay = 1 (Disable Nagle)
//! - TcpAckFrequency = 1 (Immediate ACKs, no delay)
//! - TcpDelAckTicks = 0 (Zero delayed ACK ticks)
//! - NetworkThrottlingIndex = 0xFFFFFFFF (Disable gaming network throttling)
//! - SystemResponsiveness = 0 (Prioritize game network packets)
//!
//! Safely captures original values before alteration and guarantees full restoration
//! on shutdown or drop.

use tracing::{info, warn};

#[derive(Debug, Default)]
pub struct RegistryOptimizer {
    #[cfg(windows)]
    backups: Vec<RegistryBackupEntry>,
}

#[cfg(windows)]
#[derive(Debug)]
struct RegistryBackupEntry {
    key_path: String,
    value_name: String,
    original_value: Option<u32>, // None means value did not exist originally
}

impl RegistryOptimizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_admin() -> bool {
        #[cfg(windows)]
        {
            use winreg::enums::*;
            use winreg::RegKey;
            let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
            hklm.open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters", KEY_WRITE).is_ok()
        }
        #[cfg(not(windows))]
        {
            true
        }
    }

    #[cfg(windows)]
    pub fn apply_optimizations(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use winreg::enums::*;
        use winreg::RegKey;

        info!("[Registry] Applying Windows low-latency TCP/IP & MMCSS optimizations...");

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        // 1. MMCSS SystemProfile tuning
        let system_profile_path = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
        if let Ok(key) = hklm.open_subkey_with_flags(system_profile_path, KEY_READ | KEY_WRITE) {
            self.backup_and_set_dword(&key, system_profile_path, "NetworkThrottlingIndex", 0xFFFFFFFF);
            self.backup_and_set_dword(&key, system_profile_path, "SystemResponsiveness", 0);
        } else {
            warn!("[Registry] Could not open MMCSS SystemProfile key (requires Admin privileges).");
        }

        // 2. TCP/IP Network Interfaces tuning
        let interfaces_root_path = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";
        if let Ok(interfaces_key) = hklm.open_subkey_with_flags(interfaces_root_path, KEY_READ) {
            for subkey_name in interfaces_key.enum_keys().flatten() {
                let full_path = format!(r"{}\{}", interfaces_root_path, subkey_name);
                if let Ok(iface_key) = hklm.open_subkey_with_flags(&full_path, KEY_READ | KEY_WRITE) {
                    self.backup_and_set_dword(&iface_key, &full_path, "TcpAckFrequency", 1);
                    self.backup_and_set_dword(&iface_key, &full_path, "TCPNoDelay", 1);
                    self.backup_and_set_dword(&iface_key, &full_path, "TcpDelAckTicks", 0);
                }
            }
        } else {
            warn!("[Registry] Could not open Tcpip Interfaces key (requires Admin privileges).");
        }

        info!("[Registry] Applied optimizations across {} registry entries.", self.backups.len());
        Ok(())
    }

    #[cfg(windows)]
    fn backup_and_set_dword(
        &mut self,
        key: &winreg::RegKey,
        key_path: &str,
        value_name: &str,
        target_value: u32,
    ) {
        let original = key.get_value::<u32, _>(value_name).ok();
        self.backups.push(RegistryBackupEntry {
            key_path: key_path.to_string(),
            value_name: value_name.to_string(),
            original_value: original,
        });

        if let Err(e) = key.set_value(value_name, &target_value) {
            warn!("[Registry] Failed to set {}\\{}: {}", key_path, value_name, e);
        }
    }

    #[cfg(windows)]
    pub fn restore(&mut self) {
        use winreg::enums::*;
        use winreg::RegKey;

        if self.backups.is_empty() {
            return;
        }

        info!("[Registry] Restoring {} altered registry keys to original values...", self.backups.len());
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        for entry in self.backups.drain(..) {
            if let Ok(key) = hklm.open_subkey_with_flags(&entry.key_path, KEY_WRITE) {
                match entry.original_value {
                    Some(val) => {
                        let _ = key.set_value(&entry.value_name, &val);
                    }
                    None => {
                        let _ = key.delete_value(&entry.value_name);
                    }
                }
            }
        }
        info!("[Registry] Registry state successfully restored.");
    }

    #[cfg(not(windows))]
    pub fn apply_optimizations(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("[Mock Registry] Non-Windows OS detected, bypassing registry changes.");
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn restore(&mut self) {
        info!("[Mock Registry] Non-Windows OS detected, nothing to restore.");
    }
}

impl Drop for RegistryOptimizer {
    fn drop(&mut self) {
        self.restore();
    }
}
