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

/// Helper to list attached device serials
fn get_connected_device_serials() -> Vec<String> {
    let mut serials = Vec::new();
    if let Ok(output) = Command::new("adb").args(["devices"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == "device" {
                serials.push(parts[0].to_string());
            }
        }
    }
    serials
}

/// Fast scan for open TCP ports in 30000..=50000 on target IP
async fn scan_open_ports(ip: &str) -> Vec<u16> {
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    let ip_addr: std::net::IpAddr = match ip.parse() {
        Ok(addr) => addr,
        Err(_) => return Vec::new(),
    };

    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(500));
    let mut tasks = Vec::new();

    for port in 30000..=50000u16 {
        let sem = sem.clone();
        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.ok()?;
            let socket_addr = std::net::SocketAddr::new(ip_addr, port);
            if timeout(Duration::from_millis(150), TcpStream::connect(socket_addr)).await.is_ok() {
                Some(port)
            } else {
                None
            }
        }));
    }

    let mut open_ports = Vec::new();
    for task in tasks {
        if let Ok(Some(port)) = task.await {
            open_ports.push(port);
        }
    }
    open_ports
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

    // Track devices attached before pairing
    let initial_devices = get_connected_device_serials();

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

    // Browse for pairing service
    let pairing_receiver = match mdns.browse(PAIRING_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to browse for pairing services: {}", e);
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

    println!("\n[*] Looking for device connect service...");

    // Start browsing for connect service AFTER pairing completes so mDNS sends a fresh query
    let connect_receiver = match mdns.browse(CONNECT_SERVICE) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to browse for connect services: {}", e);
            let _ = mdns.shutdown();
            return;
        }
    };

    let mut connected = false;
    let mut scanned_ports = false;
    let timeout = std::time::Instant::now();

    while running.load(Ordering::SeqCst) && !connected && timeout.elapsed() < Duration::from_secs(15) {
        // 1. Check if ADB daemon auto-connected to a new endpoint matching target IP or GUID
        let current_devices = get_connected_device_serials();
        for dev in &current_devices {
            if !initial_devices.contains(dev) {
                let clean_guid = device_guid.as_ref().map(|g| g.trim_start_matches("adb-").to_string());
                let matches_ip = device_ip.as_ref().map_or(false, |ip| dev.starts_with(ip));
                let matches_guid = device_guid.as_ref().map_or(false, |guid| dev.contains(guid))
                    || clean_guid.as_ref().map_or(false, |cg| dev.contains(cg));

                if matches_ip || matches_guid {
                    connected = true;
                    println!("[+] Auto-connected by ADB: {}", dev);
                    break;
                }
            }
        }

        if connected {
            break;
        }

        // 2. Check mDNS events for connect service matching target device
        while let Ok(event) = connect_receiver.recv_timeout(Duration::from_millis(100)) {
            if let ServiceEvent::ServiceResolved(info) = event {
                let clean_guid = device_guid.as_ref().map(|g| g.trim_start_matches("adb-").to_string());
                let matches_guid = device_guid.as_ref().map_or(false, |guid| {
                    info.get_fullname().contains(guid) || info.get_hostname().contains(guid)
                }) || clean_guid.as_ref().map_or(false, |cg| {
                    info.get_fullname().contains(cg) || info.get_hostname().contains(cg)
                });

                let matches_ip = device_ip.as_ref().map_or(false, |ip| {
                    info.get_addresses().iter().any(|a| {
                        let a_str = a.to_string();
                        a_str == *ip || format!("[{}]", a_str) == *ip
                    })
                });

                if matches_guid || matches_ip {
                    if let Some(ip) = get_preferred_ip(info.get_addresses()) {
                        let port = info.get_port();
                        let addr = format!("{}:{}", ip, port);
                        println!("[+] Connect service found matching target (mdns-sd): {}", addr);

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
        }

        if connected {
            break;
        }

        // 3. Fallback: Fast TCP port scan if mDNS didn't resolve after 3 seconds
        if !connected && !scanned_ports && timeout.elapsed() >= Duration::from_secs(3) {
            scanned_ports = true;
            if let Some(ref ip) = device_ip {
                println!("[*] Scanning active wireless debugging ports on {}...", ip);
                let open_ports = scan_open_ports(ip).await;
                for p in open_ports {
                    let addr = format!("{}:{}", ip, p);
                    println!("[*] Probing open port {}...", addr);
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

        if !connected {
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    // Cleanup
    let _ = mdns.shutdown();

    if !connected {
        // Prompt for manual port entry
        if let Some(ref ip) = device_ip {
            println!("[-] Could not auto-discover connect service for {}.", ip);
            println!("    Your device IP is: {}", ip);
            println!();
            println!("    On your device, go to Wireless Debugging settings");
            println!("    and look for the port number under 'IP address & Port' (e.g., 44061).");
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
                        if input.len() == 6 && input.chars().all(|c| c.is_ascii_digit()) {
                            println!("    Note: '{}' is a 6-digit pairing code. Port numbers are 5 digits from 'IP address & Port' (e.g. 44061).", input);
                        } else {
                            println!("    Invalid port number");
                        }
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

