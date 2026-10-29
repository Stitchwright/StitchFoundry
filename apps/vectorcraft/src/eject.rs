//! Eject the USB stick (or other removable drive) a machine folder is on, after Save for My
//! Machine, so it can be pulled out without losing the file: `diskutil eject` on macOS, udisks
//! (else GIO) on Linux, the Explorer's Eject on Windows. And the USB sticks plugged in, so Save
//! for My Machine can offer them by name.

use std::process::Command;

/// Eject the removable drive `folder` is on → the drive's name.
pub fn eject(folder: &str) -> Result<String, String> {
    platform::eject(folder)
}

/// The USB sticks (and other removable drives) plugged in now → (name, folder).
pub fn usb_sticks() -> Vec<(String, String)> {
    let mut v = platform::usb_sticks();
    v.truncate(MAX_STICKS);
    v
}

/// More drives than anyone plugs in: a cap on what a listing can return.
const MAX_STICKS: usize = 26;

const NOT_REMOVABLE: &str = "this folder isn't on a USB stick or another removable drive";

/// What a command said when it failed, else that it did.
fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("can't eject: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if why.is_empty() { "the drive couldn't be ejected: close any window showing it and try again".into() } else { why })
}

/// macOS: the volume `/Volumes/<name>` a path is on.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_volume(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/Volumes/")?;
    let name = rest.split('/').next().filter(|n| !n.is_empty())?;
    Some((format!("/Volumes/{name}"), name.to_string()))
}

/// Linux: the removable mount (`/media/…`, `/run/media/…`, `/mnt/…`) a path is on, from
/// `/proc/mounts`: (device, mount point), the longest mount point that holds the path.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_mount(path: &str, mounts: &str) -> Option<(String, String)> {
    // /proc/mounts writes a space in a name as \040
    let unescape = |s: &str| s.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\");
    mounts
        .lines()
        .filter_map(|l| {
            let mut f = l.split(' ');
            Some((f.next()?.to_string(), unescape(f.next()?)))
        })
        .filter(|(dev, mnt)| {
            dev.starts_with("/dev/")
                && ["/media/", "/run/media/", "/mnt/"].iter().any(|p| mnt.starts_with(p))
                && (path == mnt || path.starts_with(&format!("{mnt}/")))
        })
        .max_by_key(|(_, mnt)| mnt.len())
}

/// macOS: the volumes in /Volumes that are drives plugged in: not the startup disk (a link to /),
/// hidden ones or the system's own (`com.apple.…`). `entries`: (name, is a link).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mac_sticks(entries: impl IntoIterator<Item = (String, bool)>) -> Vec<(String, String)> {
    entries
        .into_iter()
        .filter(|(name, link)| !link && !name.is_empty() && !name.starts_with('.') && !name.starts_with("com.apple"))
        .map(|(name, _)| (name.clone(), format!("/Volumes/{name}")))
        .collect()
}

/// Linux: the removable mounts in `/proc/mounts` (a device mounted under `/media/` or
/// `/run/media/`, as the desktop mounts a USB stick) → (name, mount point).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_sticks(mounts: &str) -> Vec<(String, String)> {
    let unescape = |s: &str| s.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\");
    mounts
        .lines()
        .filter_map(|l| {
            let mut f = l.split(' ');
            Some((f.next()?.to_string(), unescape(f.next()?)))
        })
        .filter(|(dev, mnt)| dev.starts_with("/dev/") && ["/media/", "/run/media/"].iter().any(|p| mnt.starts_with(p)))
        .map(|(_, mnt)| {
            let name = std::path::Path::new(&mnt).file_name().map_or_else(|| mnt.clone(), |n| n.to_string_lossy().into_owned());
            (name, mnt)
        })
        .collect()
}

/// Windows: the removable drives PowerShell listed, a line each as `E:|LABEL` → (name, `E:\`),
/// named after the label, else the letter.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_sticks(listing: &str) -> Vec<(String, String)> {
    listing
        .lines()
        .filter_map(|l| {
            let (drive, label) = l.trim().split_once('|').unwrap_or((l.trim(), ""));
            let mut c = drive.chars();
            let ok = c.next().is_some_and(|x| x.is_ascii_alphabetic()) && c.next() == Some(':') && c.next().is_none();
            ok.then(|| {
                let label = label.trim();
                (if label.is_empty() { drive.to_string() } else { format!("{label} ({drive})") }, format!("{drive}\\"))
            })
        })
        .collect()
}

/// Windows: the drive letter a path is on (`E:`), unless it is the system drive.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_drive(path: &str, system: &str) -> Option<String> {
    let mut c = path.chars();
    let (letter, colon) = (c.next()?, c.next()?);
    let drive = format!("{}:", letter.to_ascii_uppercase());
    (letter.is_ascii_alphabetic() && colon == ':' && !drive.eq_ignore_ascii_case(system.trim_end_matches('\\'))).then_some(drive)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn eject(folder: &str) -> Result<String, String> {
        let (volume, name) = mac_volume(folder).ok_or(NOT_REMOVABLE)?;
        run(Command::new("diskutil").args(["eject", &volume]))?;
        Ok(name)
    }

    pub fn usb_sticks() -> Vec<(String, String)> {
        let Ok(dir) = std::fs::read_dir("/Volumes") else { return Vec::new() };
        mac_sticks(dir.flatten().take(64).map(|e| (e.file_name().to_string_lossy().into_owned(), e.file_type().is_ok_and(|t| t.is_symlink()))))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Command, NOT_REMOVABLE, linux_mount, linux_sticks, run};

    pub fn eject(folder: &str) -> Result<String, String> {
        let mounts = std::fs::read_to_string("/proc/mounts").map_err(|e| format!("can't eject: {e}"))?;
        let (device, mount) = linux_mount(folder, &mounts).ok_or(NOT_REMOVABLE)?;
        let name = std::path::Path::new(&mount).file_name().map_or_else(|| mount.clone(), |n| n.to_string_lossy().into_owned());
        // udisks unmounts and powers the stick off, as the file manager's Eject does; without it, GIO
        match run(Command::new("udisksctl").args(["unmount", "--block-device", &device])) {
            Ok(()) => {
                // the stick is safe once unmounted; powering it off as well can fail harmlessly
                let _ = run(Command::new("udisksctl").args(["power-off", "--block-device", &device]));
            }
            Err(e) if e.starts_with("can't eject") => run(Command::new("gio").args(["mount", "--eject", &mount]))?,
            Err(e) => return Err(e),
        }
        Ok(name)
    }

    pub fn usb_sticks() -> Vec<(String, String)> {
        std::fs::read_to_string("/proc/mounts").map(|m| linux_sticks(&m)).unwrap_or_default()
    }
}

#[cfg(windows)]
mod platform {
    use super::*;

    pub fn eject(folder: &str) -> Result<String, String> {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let drive = windows_drive(folder, &system).ok_or(NOT_REMOVABLE)?;
        // the Explorer's Eject on the drive (shell namespace 17: This PC)
        let script = format!("(New-Object -ComObject Shell.Application).Namespace(17).ParseName('{drive}').InvokeVerb('Eject')");
        run(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]))?;
        Ok(drive)
    }

    pub fn usb_sticks() -> Vec<(String, String)> {
        // removable drives (drive type 2): the letter and the label
        let script = "Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=2' | ForEach-Object { $_.DeviceID + '|' + $_.VolumeName }";
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        {
            // no console window flashing up
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        match cmd.output() {
            Ok(out) if out.status.success() => windows_sticks(&String::from_utf8_lossy(&out.stdout)),
            _ => Vec::new(),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    pub fn eject(_folder: &str) -> Result<String, String> {
        Err("ejecting isn't available on this system: use the file manager's Eject".into())
    }

    pub fn usb_sticks() -> Vec<(String, String)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_removable_drive_a_folder_is_on() {
        assert_eq!(mac_volume("/Volumes/NO NAME/designs"), Some(("/Volumes/NO NAME".into(), "NO NAME".into())));
        assert_eq!(mac_volume("/Volumes/STICK"), Some(("/Volumes/STICK".into(), "STICK".into())));
        assert_eq!(mac_volume("/Users/ann/Embroidery"), None);
        assert_eq!(mac_volume("/Volumes/"), None);
        let mounts = "/dev/nvme0n1p2 / ext4 rw 0 0\n/dev/sdb1 /media/ann/NO\\040NAME vfat rw 0 0\n/dev/sdc1 /run/media/ann/STICK vfat rw 0 0\ntmpfs /media/ann/tmp tmpfs rw 0 0\n";
        assert_eq!(linux_mount("/media/ann/NO NAME/designs", mounts), Some(("/dev/sdb1".into(), "/media/ann/NO NAME".into())));
        assert_eq!(linux_mount("/run/media/ann/STICK", mounts), Some(("/dev/sdc1".into(), "/run/media/ann/STICK".into())));
        assert_eq!(linux_mount("/home/ann", mounts), None, "the system disk");
        assert_eq!(linux_mount("/media/ann/tmp/x", mounts), None, "not a device");
        assert_eq!(linux_mount("/media/ann/NO NAMES/x", mounts), None, "a prefix of the name isn't the folder");
        assert_eq!(windows_drive("E:\\Designs", "C:"), Some("E:".into()));
        assert_eq!(windows_drive("e:/Designs", "C:"), Some("E:".into()));
        assert_eq!(windows_drive("C:\\Users\\ann", "C:"), None, "the system drive");
        assert_eq!(windows_drive("\\\\server\\share", "C:"), None);
    }

    #[test]
    fn the_usb_sticks_plugged_in() {
        let mac = mac_sticks([
            ("Macintosh HD".to_string(), true),
            ("NO NAME".to_string(), false),
            (".hidden".to_string(), false),
            ("com.apple.TimeMachine.localsnapshots".to_string(), false),
        ]);
        assert_eq!(mac, vec![("NO NAME".to_string(), "/Volumes/NO NAME".to_string())]);
        let mounts = "/dev/nvme0n1p2 / ext4 rw 0 0\n/dev/sdb1 /media/ann/NO\\040NAME vfat rw 0 0\n/dev/sdc1 /run/media/ann/STICK vfat rw 0 0\ntmpfs /media/ann/tmp tmpfs rw 0 0\n/dev/sda2 /mnt/backup ext4 rw 0 0\n";
        assert_eq!(
            linux_sticks(mounts),
            vec![("NO NAME".to_string(), "/media/ann/NO NAME".to_string()), ("STICK".to_string(), "/run/media/ann/STICK".to_string())]
        );
        assert_eq!(
            windows_sticks("E:|KINGSTON\r\nF:|\r\n\r\njunk\r\n"),
            vec![("KINGSTON (E:)".to_string(), "E:\\".to_string()), ("F:".to_string(), "F:\\".to_string())]
        );
    }
}
