//! ADB QR Code Pairing Tool
//!
//! Generates a QR code that your Android device can scan to pair for wireless debugging.
//! Based on: https://gist.github.com/benigumocom/a6a87fc1cb690c3c4e3a7642ebf2be6f

use mdns_sd::{ServiceDaemon, ServiceEvent};
use qrcode::QrCode;
use rand::Rng;
use std::net::IpAddr;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// mDNS service type for ADB pairing
const PAIRING_SERVICE: &str = "_adb-tls-pairing._tcp.local.";

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

/// Get the preferred IP address (IPv4 over IPv6) and format it for ADB
fn get_preferred_ip(addresses: &std::collections::HashSet<IpAddr>) -> Option<String> {
    let addresses: Vec<_> = addresses.iter().collect();
    let addr = addresses
        .iter()
        .find(|a| a.is_ipv4())
        .or(addresses.first())
        .copied()?;

    Some(if addr.is_ipv6() {
        format!("[{}]", addr)
    } else {
        addr.to_string()
    })
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
    println!("  pairqr v{} - ADB Wireless Debugging", env!("CARGO_PKG_VERSION"));
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

    // Browse for pairing services
    let pairing_receiver = match mdns.browse(PAIRING_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to browse for pairing services: {}", e);
            return;
        }
    };

    let mut paired = false;

    // Wait for pairing
    while running.load(Ordering::SeqCst) && !paired {
        match pairing_receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => match event {
                ServiceEvent::ServiceResolved(info) => {
                    println!("\n[+] Device found: {}", info.get_fullname());

                    if let Some(ip) = get_preferred_ip(info.get_addresses()) {
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

    // Shutdown our mDNS - we'll use ADB's built-in mdns instead
    let _ = mdns.shutdown();

    // Wait a moment for the device to settle after pairing dialog closes
    println!("\n[*] Looking for device connect service...");
    std::thread::sleep(Duration::from_millis(500));

    // Try to find connect service using ADB's built-in mdns
    let mut connected = false;
    let timeout = std::time::Instant::now();

    while running.load(Ordering::SeqCst) && !connected && timeout.elapsed() < Duration::from_secs(15) {
        if let Ok(output) = Command::new("adb").args(["mdns", "services"]).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Parse adb mdns services output for connect services
            // Format: "adb-SERIAL-XXXXXX	_adb-tls-connect._tcp.	IP:PORT"
            for line in stdout.lines() {
                if line.contains("_adb-tls-connect._tcp") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let addr = parts[2]; // IP:PORT
                        println!("[+] Connect service found: {}", addr);

                        // Try to connect
                        let connect_output = Command::new("adb")
                            .args(["connect", addr])
                            .output();

                        if let Ok(result) = connect_output {
                            let out = String::from_utf8_lossy(&result.stdout);
                            if out.contains("connected") || out.contains("already") {
                                println!("[+] Connected successfully!");
                                connected = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        if !connected {
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    if !connected {
        println!("[-] Could not auto-connect. You may need to connect manually.");
        println!("    Check 'Wireless Debugging' on your device for the IP & port,");
        println!("    then run: adb connect <ip>:<port>");
    }

    show_devices();
}
