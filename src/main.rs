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

/// mDNS service type for ADB connect
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

/// Run adb pair command - returns (success, Option<guid>)
fn adb_pair(ip: &str, port: u16, password: &str) -> (bool, Option<String>) {
    println!("\n[*] Running: adb pair {}:{} ******", ip, port);

    let output = Command::new("adb")
        .args(["pair", &format!("{}:{}", ip, port), password])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);

            // Print actual output for debugging
            if !stdout.trim().is_empty() {
                println!("    adb: {}", stdout.trim());
            }
            if !stderr.trim().is_empty() {
                println!("    adb err: {}", stderr.trim());
            }

            if stdout.contains("Successfully paired") {
                println!("[+] Pairing successful!");

                // Extract GUID from output: "Successfully paired to IP:PORT [guid=XXX]"
                let guid = stdout
                    .split("[guid=")
                    .nth(1)
                    .and_then(|s| s.split(']').next())
                    .map(|s| s.to_string());

                (true, guid)
            } else {
                println!("[-] Pairing may have failed");
                (false, None)
            }
        }
        Err(e) => {
            println!("[-] Failed to run adb: {}", e);
            (false, None)
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

    // Browse for both pairing and connect services from the start
    let pairing_receiver = match mdns.browse(PAIRING_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to browse for pairing services: {}", e);
            return;
        }
    };

    let connect_receiver = match mdns.browse(CONNECT_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to browse for connect services: {}", e);
            return;
        }
    };

    let mut paired = false;
    let mut device_guid: Option<String> = None;
    let mut device_ip: Option<String> = None;

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

                        let (success, guid) = adb_pair(&ip, port, &password);
                        if success {
                            paired = true;
                            device_guid = guid;
                            device_ip = Some(ip);
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

    // Wait a moment for the device to settle after pairing dialog closes
    println!("\n[*] Looking for device connect service...");

    let mut connected = false;
    let timeout = std::time::Instant::now();

    // If we have a GUID, try to resolve the specific service using dns-sd (macOS)
    if let Some(ref guid) = device_guid {
        let service_name = guid.clone();
        println!("[*] Looking for service: {}", service_name);

        // Use dns-sd -L to resolve the specific service (with timeout via spawn)
        if let Ok(mut child) = Command::new("dns-sd")
            .args(["-L", &service_name, "_adb-tls-connect._tcp", "local."])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            // Give dns-sd time to resolve
            std::thread::sleep(Duration::from_secs(3));

            // Kill it and read output
            let _ = child.kill();
            if let Ok(output) = child.wait_with_output() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse output for port: "can be reached at hostname:port"
                for line in stdout.lines() {
                    if line.contains("can be reached at") {
                        if let Some(addr_part) = line.split("can be reached at ").nth(1) {
                            // Format: "hostname.local.:PORT (interface X)"
                            if let Some(port_str) = addr_part.split(':').nth(1) {
                                if let Some(port_part) = port_str.split_whitespace().next() {
                                    if let Ok(port) = port_part.parse::<u16>() {
                                        if let Some(ref ip) = device_ip {
                                            let addr = format!("{}:{}", ip, port);
                                            println!("[+] Connect service resolved (dns-sd): {}", addr);

                                            if let Ok(result) = Command::new("adb").args(["connect", &addr]).output() {
                                                let out = String::from_utf8_lossy(&result.stdout);
                                                println!("    adb: {}", out.trim());
                                                if out.contains("connected") || out.contains("already") {
                                                    println!("[+] Connected!");
                                                    connected = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: try other discovery methods
    while running.load(Ordering::SeqCst) && !connected && timeout.elapsed() < Duration::from_secs(15) {
        // Check our own mDNS browse
        while let Ok(event) = connect_receiver.recv_timeout(Duration::from_millis(0)) {
            if let ServiceEvent::ServiceResolved(info) = event {
                if let Some(ip) = get_preferred_ip(info.get_addresses()) {
                    let port = info.get_port();
                    let addr = format!("{}:{}", ip, port);
                    println!("[+] Connect service found (mdns-sd): {}", addr);

                    if let Ok(result) = Command::new("adb").args(["connect", &addr]).output() {
                        let out = String::from_utf8_lossy(&result.stdout);
                        println!("    adb: {}", out.trim());
                        if out.contains("connected") || out.contains("already") {
                            println!("[+] Connected!");
                            connected = true;
                            break;
                        }
                    }
                }
            }
        }

        // Check ADB's built-in mdns
        if !connected {
            if let Ok(output) = Command::new("adb").args(["mdns", "services"]).output() {
                let stdout = String::from_utf8_lossy(&output.stdout);

                for line in stdout.lines() {
                    if line.contains("_adb-tls-connect._tcp") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 {
                            let addr = parts[2];
                            println!("[+] Connect service found (adb mdns): {}", addr);

                            if let Ok(result) = Command::new("adb").args(["connect", addr]).output() {
                                let out = String::from_utf8_lossy(&result.stdout);
                                println!("    adb: {}", out.trim());
                                if out.contains("connected") || out.contains("already") {
                                    println!("[+] Connected!");
                                    connected = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !connected {
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    // Cleanup
    let _ = mdns.shutdown();

    if !connected {
        // Prompt for manual port entry
        if let Some(ref ip) = device_ip {
            println!("[-] Could not auto-discover connect service.");
            println!("    Your device IP is: {}", ip);
            println!();
            println!("    On your device, go to Wireless Debugging settings");
            println!("    and look for the port number (e.g., 'IP address & Port').");
            println!();
            print!("    Enter the port number (or press Enter to skip): ");
            use std::io::{self, Write};
            let _ = io::stdout().flush();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_ok() {
                let input = input.trim();
                if !input.is_empty() {
                    if let Ok(port) = input.parse::<u16>() {
                        let addr = format!("{}:{}", ip, port);
                        println!("[*] Connecting to {}...", addr);

                        if let Ok(result) = Command::new("adb").args(["connect", &addr]).output() {
                            let out = String::from_utf8_lossy(&result.stdout);
                            println!("    adb: {}", out.trim());
                            if out.contains("connected") || out.contains("already") {
                                println!("[+] Connected!");
                                connected = true;
                            }
                        }
                    } else {
                        println!("    Invalid port number");
                    }
                }
            }
        }

        if !connected {
            println!("[-] Not connected. Run manually: adb connect <ip>:<port>");
        }
    }

    show_devices();
}
