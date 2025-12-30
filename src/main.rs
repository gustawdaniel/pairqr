//! ADB QR Code Pairing Tool
//!
//! Generates a QR code that your Android device can scan to pair for wireless debugging.
//! Based on: https://gist.github.com/benigumocom/a6a87fc1cb690c3c4e3a7642ebf2be6f

use mdns_sd::{ServiceDaemon, ServiceEvent};
use qrcode::QrCode;
use rand::Rng;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// mDNS service type for ADB pairing
const PAIRING_SERVICE: &str = "_adb-tls-pairing._tcp.local.";

/// mDNS service type for ADB connect (after pairing)
const CONNECT_SERVICE: &str = "_adb-tls-connect._tcp.local.";

/// Characters for random name generation
const NAME_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Characters for random password generation
const PASS_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%";

/// Generate a random string from the given character set
fn random_string(chars: &[u8], length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..chars.len());
            chars[idx] as char
        })
        .collect()
}

/// Display QR code in terminal using half-block Unicode characters
/// This packs 2 QR rows into 1 terminal row, producing a compact output like Python's qrcode library
fn display_qr_code(data: &str) {
    let code = match QrCode::new(data) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to generate QR code: {}", e);
            return;
        }
    };

    let colors = code.to_colors();
    let width = code.width();

    // Quiet zone (border) in modules
    let border = 2;

    // Helper to check if a module is dark (treating out-of-bounds as light for border)
    let is_dark = |row: isize, col: isize| -> bool {
        if row < 0 || col < 0 || row >= width as isize || col >= width as isize {
            false
        } else {
            colors[row as usize * width + col as usize] == qrcode::Color::Dark
        }
    };

    // Process 2 QR rows at a time using half-block characters
    // ▀ (upper half) = top dark, bottom light
    // ▄ (lower half) = top light, bottom dark
    // █ (full block) = both dark
    // ' ' (space) = both light
    let total_rows = width + border * 2;
    let total_cols = width + border * 2;

    for row_pair in (0..total_rows).step_by(2) {
        for col in 0..total_cols {
            let qr_row_top = row_pair as isize - border as isize;
            let qr_row_bot = qr_row_top + 1;
            let qr_col = col as isize - border as isize;

            let top_dark = is_dark(qr_row_top, qr_col);
            let bot_dark = is_dark(qr_row_bot, qr_col);

            // Invert: dark QR modules should appear dark (background), light modules should appear light (foreground)
            // On dark terminal: space = dark background, block = light foreground
            let ch = match (top_dark, bot_dark) {
                (true, true) => ' ',   // both dark → background
                (true, false) => '▄',  // top dark (bg), bottom light (fg) → lower half block
                (false, true) => '▀',  // top light (fg), bottom dark (bg) → upper half block
                (false, false) => '█', // both light → full block
            };
            print!("{}", ch);
        }
        println!();
    }
}

/// Run adb pair command
fn adb_pair(ip: &str, port: u16, password: &str) -> bool {
    println!("\n[*] Running: adb pair {}:{} ******", ip, port);

    let output = Command::new("adb")
        .args(["pair", &format!("{}:{}", ip, port), password])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);

            if result.status.success() || stdout.contains("Successfully paired") {
                println!("[+] Pairing successful!");
                true
            } else {
                println!(
                    "[-] Pairing failed: {}",
                    if stderr.is_empty() { &stdout } else { &stderr }
                );
                false
            }
        }
        Err(e) => {
            println!("[-] Failed to run adb: {}", e);
            false
        }
    }
}

/// Run adb connect command
fn adb_connect(ip: &str, port: u16) -> bool {
    println!("[*] Running: adb connect {}:{}", ip, port);

    let output = Command::new("adb")
        .args(["connect", &format!("{}:{}", ip, port)])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);

            if result.status.success() && (stdout.contains("connected") || stdout.contains("already")) {
                println!("[+] Connected successfully!");
                true
            } else {
                println!(
                    "[-] Connect failed: {}",
                    if stderr.is_empty() { &stdout } else { &stderr }
                );
                false
            }
        }
        Err(e) => {
            println!("[-] Failed to run adb: {}", e);
            false
        }
    }
}

/// Show connected devices
fn show_devices() {
    println!("\n[*] Connected devices:");
    let _ = Command::new("adb").args(["devices", "-l"]).status();
}

#[tokio::main]
async fn main() {
    // Generate random credentials (like Android Studio does)
    let name = format!("studio-{}", random_string(NAME_CHARS, 10));
    let password = random_string(PASS_CHARS, 10);

    // QR code format (same as Android Studio)
    let qr_text = format!("WIFI:T:ADB;S:{};P:{};;", name, password);

    println!("{}", "=".repeat(50));
    println!("  ADB Wireless Debugging - QR Code Pairing");
    println!("{}", "=".repeat(50));
    println!();

    // Display QR code
    display_qr_code(&qr_text);

    println!();
    println!("On your Android device:");
    println!("  1. Settings > Developer Options > Wireless Debugging");
    println!("  2. Tap 'Pair device with QR code'");
    println!("  3. Scan the QR code above");
    println!();
    println!("[*] Waiting for device to scan QR code...");
    println!("    (Press Ctrl+C to exit)");
    println!();

    // Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Create mDNS service daemon
    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to create mDNS daemon: {}", e);
            return;
        }
    };

    // Browse for ADB pairing services
    let receiver = match mdns.browse(PAIRING_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to browse for services: {}", e);
            return;
        }
    };

    let mut paired = false;

    // Main event loop
    while running.load(Ordering::SeqCst) && !paired {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => match event {
                ServiceEvent::ServiceResolved(info) => {
                    println!("\n[+] Device found: {}", info.get_fullname());

                    // Prefer IPv4 addresses over IPv6 (ADB has issues with IPv6 link-local)
                    let addresses: Vec<_> = info.get_addresses().iter().collect();
                    let addr = addresses
                        .iter()
                        .find(|a| a.is_ipv4())
                        .or(addresses.first())
                        .copied();

                    if let Some(addr) = addr {
                        // Format IPv6 addresses with brackets for adb
                        let ip = if addr.is_ipv6() {
                            format!("[{}]", addr)
                        } else {
                            addr.to_string()
                        };
                        let port = info.get_port();

                        println!("    Server: {}", info.get_hostname());
                        println!("    Port: {}", port);

                        if adb_pair(&ip, port, &password) {
                            paired = true;
                        }
                    }
                }
                ServiceEvent::ServiceRemoved(_type, name) => {
                    println!("\n[!] Service removed: {}", name);
                }
                _ => {}
            },
            Err(flume::RecvTimeoutError::Timeout) => continue,
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }

    if !running.load(Ordering::SeqCst) {
        println!("\n\n[*] Cancelled by user");
        let _ = mdns.shutdown();
        return;
    }

    if !paired {
        let _ = mdns.shutdown();
        return;
    }

    // Now discover and connect to the device's connect service
    println!("\n[*] Looking for device connect service...");

    let connect_receiver = match mdns.browse(CONNECT_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to browse for connect services: {}", e);
            let _ = mdns.shutdown();
            return;
        }
    };

    let mut connected = false;
    let timeout = std::time::Instant::now();

    while running.load(Ordering::SeqCst) && !connected && timeout.elapsed() < Duration::from_secs(10) {
        match connect_receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                if let ServiceEvent::ServiceResolved(info) = event {
                    // Prefer IPv4 addresses
                    let addresses: Vec<_> = info.get_addresses().iter().collect();
                    let addr = addresses
                        .iter()
                        .find(|a| a.is_ipv4())
                        .or(addresses.first())
                        .copied();

                    if let Some(addr) = addr {
                        let ip = if addr.is_ipv6() {
                            format!("[{}]", addr)
                        } else {
                            addr.to_string()
                        };
                        let port = info.get_port();

                        println!("\n[+] Connect service found: {}", info.get_fullname());
                        println!("    Port: {}", port);

                        if adb_connect(&ip, port) {
                            connected = true;
                        }
                    }
                }
            }
            Err(flume::RecvTimeoutError::Timeout) => continue,
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }

    if !connected {
        println!("[-] Could not auto-connect. You may need to connect manually.");
        println!("    Check 'Wireless Debugging' on your device for the IP & port,");
        println!("    then run: adb connect <ip>:<port>");
    }

    // Cleanup
    let _ = mdns.shutdown();

    show_devices();
}
