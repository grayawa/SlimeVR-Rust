//! ESP8266 ROM flasher. ESP32 variants use espflash's native stub and verification.
//! Packet layout and the ROM sector-erase workaround follow Espressif's esptool protocol.
use serialport::SerialPort;
use std::{
    io::Write,
    time::{Duration, Instant},
};
const BLOCK: usize = 1024;
pub fn encode(op: u8, value: u32, data: &[u8]) -> Vec<u8> {
    let mut packet = vec![0, op];
    packet.extend_from_slice(&(data.len() as u16).to_le_bytes());
    packet.extend_from_slice(&value.to_le_bytes());
    packet.extend_from_slice(data);
    let mut slip = vec![0xc0];
    for byte in packet {
        match byte {
            0xc0 => slip.extend_from_slice(&[0xdb, 0xdc]),
            0xdb => slip.extend_from_slice(&[0xdb, 0xdd]),
            b => slip.push(b),
        }
    }
    slip.push(0xc0);
    slip
}
fn receive(port: &mut dyn SerialPort, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut packet = Vec::new();
    let mut escaped = false;
    let mut started = false;
    let mut byte = [0];
    while Instant::now() < deadline {
        match port.read(&mut byte) {
            Ok(0) => continue,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
        match byte[0] {
            0xc0 if started && !packet.is_empty() => return Ok(packet),
            0xc0 => {
                started = true;
                escaped = false;
            }
            _ if !started => {}
            0xdb => escaped = true,
            b => {
                let decoded = if escaped {
                    escaped = false;
                    match b {
                        0xdc => 0xc0,
                        0xdd => 0xdb,
                        _ => return Err("invalid ESP SLIP escape".into()),
                    }
                } else {
                    b
                };
                packet.push(decoded);
                if packet.len() > 65544 {
                    return Err("oversized ESP ROM response".into());
                }
            }
        }
    }
    Err("ESP ROM response timed out".into())
}
fn request(
    port: &mut dyn SerialPort,
    op: u8,
    value: u32,
    data: &[u8],
    timeout: Duration,
) -> Result<u32, String> {
    port.write_all(&encode(op, value, data))
        .map_err(|e| e.to_string())?;
    port.flush().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    loop {
        let reply = receive(port, deadline)?;
        if reply.len() < 10 || reply[0] != 1 || reply[1] != op {
            continue;
        }
        let len = u16::from_le_bytes([reply[2], reply[3]]) as usize;
        if len + 8 != reply.len() || len < 2 {
            return Err("invalid ESP ROM response size".into());
        }
        if reply[reply.len() - 2] != 0 {
            return Err(format!(
                "ESP ROM command {op:#x} rejected: {}",
                reply[reply.len() - 1]
            ));
        }
        return Ok(u32::from_le_bytes(reply[4..8].try_into().unwrap()));
    }
}
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
pub fn erase_size(size: usize, offset: u32) -> u32 {
    let sectors = size.div_ceil(4096) as u32;
    let head = (16 - (offset / 4096) % 16).min(sectors);
    if sectors < 2 * head {
        sectors.div_ceil(2) * 4096
    } else {
        (sectors - head) * 4096
    }
}
/// Return false after a successful ROM sync if this is another ESP family.
pub fn try_flash(port: &mut dyn SerialPort, parts: &[(u32, &[u8])]) -> Result<bool, String> {
    port.set_timeout(Duration::from_millis(100))
        .map_err(|e| e.to_string())?;
    port.write_data_terminal_ready(false)
        .map_err(|e| e.to_string())?;
    port.write_request_to_send(true)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(100));
    port.write_data_terminal_ready(true)
        .map_err(|e| e.to_string())?;
    port.write_request_to_send(false)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(50));
    port.write_data_terminal_ready(false)
        .map_err(|e| e.to_string())?;
    let _ = port.clear(serialport::ClearBuffer::All);
    let mut sync = vec![7, 7, 0x12, 0x20];
    sync.extend([0x55; 32]);
    let mut synced = false;
    for _ in 0..4 {
        if request(port, 8, 0, &sync, Duration::from_millis(700)).is_ok() {
            synced = true;
            break;
        }
    }
    if !synced {
        return Ok(false);
    }
    let _ = port.clear(serialport::ClearBuffer::All);
    let magic = request(port, 0x0a, 0, &words(&[0x60000078]), Duration::from_secs(2))?;
    if magic != 0xfff0c101 {
        return Ok(false);
    }
    let total: usize = parts.iter().map(|(_, data)| data.len()).sum();
    let mut uploaded = 0;
    for (offset, bytes) in parts {
        let count = bytes.len().div_ceil(BLOCK);
        request(
            port,
            2,
            0,
            &words(&[
                erase_size(bytes.len(), *offset),
                count as u32,
                BLOCK as u32,
                *offset,
            ]),
            Duration::from_secs(30),
        )?;
        for (seq, chunk) in bytes.chunks(BLOCK).enumerate() {
            let mut payload = vec![0xff; BLOCK];
            payload[..chunk.len()].copy_from_slice(chunk);
            let checksum = payload.iter().fold(0xef, |a, b| a ^ b) as u32;
            let mut data = words(&[BLOCK as u32, seq as u32, 0, 0]);
            data.extend(payload);
            request(port, 3, checksum, &data, Duration::from_secs(5))?;
            uploaded += chunk.len();
            println!("{}", uploaded * 100 / total.max(1));
            let _ = std::io::stdout().flush();
        }
    }
    request(port, 4, 0, &words(&[0]), Duration::from_secs(3))?;
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slip_and_rom_erase_workaround() {
        assert_eq!(
            encode(3, 0xef, &[0xc0, 0xdb]),
            vec![0xc0, 0, 3, 2, 0, 0xef, 0, 0, 0, 0xdb, 0xdc, 0xdb, 0xdd, 0xc0]
        );
        assert_eq!(erase_size(1, 0), 4096);
        assert_eq!(erase_size(4096 * 16, 0), 4096 * 8);
        assert_eq!(erase_size(4096 * 32, 0), 4096 * 16);
        assert_eq!(erase_size(4096 * 16, 4096 * 15), 4096 * 15);
    }
}
