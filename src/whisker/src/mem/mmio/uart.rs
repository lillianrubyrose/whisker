use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::fd::{AsFd, RawFd};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::{env, thread};

use command_fds::{CommandFdExt as _, FdMapping};
use num_conv::Truncate;
use socketpair::socketpair_stream;

use tracing::{debug, error, warn};

use crate::cpu::hart::WhiskerHart;
use crate::cpu::interrupts::InterruptEvent;
use crate::mem::mmio::MMIODevice;

pub const UART_BASE: u64 = 0x1000_0000;

// several of these registers are reused
pub const DATA_REG: u64 = UART_BASE;
pub const INTERRUPT_ENABLE_REG: u64 = UART_BASE + 1;
pub const INTERRUPT_IDENT_REG: u64 = UART_BASE + 2;
pub const FIFO_CONTROL_REG: u64 = UART_BASE + 2;
pub const LINE_CONTROL_REG: u64 = UART_BASE + 3;
pub const MODEM_CONTROL_REG: u64 = UART_BASE + 4;
pub const LINE_STATUS_REG: u64 = UART_BASE + 5;
pub const MODEM_STATUS_REG: u64 = UART_BASE + 6;
pub const SCRATCH_REG: u64 = UART_BASE + 7;

pub struct UART {
	data_reg: u8,
	interrupt_enable: UartInterruptKind,
	line_control_reg: u8,

	scratch_reg: u8,

	divisor: u16,

	data_queue: VecDeque<u8>,

	stdout: Box<dyn Write + Send + Sync>,
	interrupt_tx: Sender<InterruptEvent>,
}

impl UART {
	pub fn init(interrupt_tx: Sender<InterruptEvent>) -> Arc<Mutex<Self>> {
		const REMOTE_FD_NUM: RawFd = 4;
		let (mut local, other) = socketpair_stream().expect("unable to create socket pair");
		let mut cmd = Command::new(env::var_os("TERM").expect("could not find $TERM"));
		cmd.args([
			"--hold",
			"-e",
			"socat",
			"stdio,raw,echo=0,icrnl,opost",
			format!("FD:{REMOTE_FD_NUM},crnl").as_str(),
		])
		.fd_mappings(vec![FdMapping {
			parent_fd: other.as_fd().try_clone_to_owned().expect("unable to clone fd for uart"),
			child_fd: REMOTE_FD_NUM,
		}])
		.expect("fd collision");
		let _ = cmd.spawn().expect("unable to spawn UART terminal");

		let this = Arc::new(Mutex::new(Self {
			data_reg: 0,
			line_control_reg: 0b00000011, // no parity, 1 stop, 8 data
			interrupt_enable: UartInterruptKind::empty(),

			scratch_reg: 0,

			divisor: 1,

			data_queue: VecDeque::new(),
			stdout: Box::new(local.try_clone().unwrap()),
			interrupt_tx,
		}));

		thread::spawn({
			let uart = Arc::clone(&this);
			move || 'outer: loop {
				let mut b = 0_u8;
				let Ok(()) = local.read_exact(core::slice::from_mut(&mut b)) else {
					error!("reading from UART term failed");
					break 'outer;
				};

				debug!("UART read: {:0X}", b);

				let mut uart = uart.lock().unwrap();
				uart.data_queue.push_back(b);
				if uart.interrupt_enable.contains(UartInterruptKind::RX_DATA_AVAILABLE) {
					uart.interrupt_tx
						.send(InterruptEvent::UartInterrupt)
						.expect("could not send to interrupt controller");
				}
			}
		});

		this
	}
}

impl MMIODevice for UART {
	fn read(&mut self, _: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		let is_dlab = self.line_control_reg & 0b1000_0000 == 1;
		match addr {
			DATA_REG => {
				if is_dlab {
					// divisor LSB
					buf[0] = (self.divisor & 0xFF).truncate();
				} else {
					buf[0] = self.do_read();
				}
			}
			INTERRUPT_ENABLE_REG => {
				if is_dlab {
					// divisor MSB
					buf[0] = (self.divisor >> 8).truncate();
				} else {
					buf[0] = self.interrupt_enable.bits();
				}
			}
			INTERRUPT_IDENT_REG => todo!("read interrupt ident register"),
			LINE_CONTROL_REG => buf[0] = self.line_control_reg,
			MODEM_CONTROL_REG => todo!("read modem control register"),
			LINE_STATUS_REG => buf[0] = self.read_line_status(),
			MODEM_STATUS_REG => todo!("read modem status register"),
			SCRATCH_REG => buf[0] = self.scratch_reg,
			_ => warn!("read from unknown UART addr {:#018X}", addr),
		}
	}

	fn write(&mut self, _: &mut WhiskerHart, addr: u64, val: &[u8]) {
		let is_dlab = self.line_control_reg & 0b1000_0000 == 1;
		match addr {
			DATA_REG => {
				if is_dlab {
					// divisor LSB
					self.divisor &= 0xFF00;
					self.divisor |= u16::from(val[0]);
				} else {
					self.do_write(val[0]);
				}
			}
			INTERRUPT_ENABLE_REG => {
				if is_dlab {
					//divisor MSB
					self.divisor &= 0x00FF;
					self.divisor |= u16::from(val[0]) << 8;
				} else {
					self.interrupt_enable = UartInterruptKind::from_bits_retain(val[0]);
				}
			}
			FIFO_CONTROL_REG => todo!("write FIFO queue settings"),
			LINE_CONTROL_REG => self.write_line_control(val[0]),
			MODEM_CONTROL_REG => todo!("write modem control reg"),
			LINE_STATUS_REG => {} // ignored
			MODEM_STATUS_REG => todo!("write modem status register"),
			SCRATCH_REG => self.scratch_reg = val[0],
			_ => {
				warn!("write to unknown UART addr {:#018X}", addr);
			}
		}
	}
}

impl UART {
	fn do_read(&mut self) -> u8 {
		if let Some(val) = self.data_queue.pop_front() {
			self.data_reg = val;
		}
		self.data_reg
	}

	fn do_write(&mut self, val: u8) {
		self.data_reg = val;
		let _ = write!(&mut self.stdout, "{}", self.data_reg as char);
		// FIXME: it would be nice to flush stdout all the time but its VERY slow, reconsider this
		let _ = self.stdout.flush();

		// the transmitter register is considered to immedately be empty
		if self.interrupt_enable.contains(UartInterruptKind::TX_REG_EMPTY) {
			self.interrupt_tx
				.send(InterruptEvent::UartInterrupt)
				.expect("could not send to interrupt controller");
		}
	}

	fn write_line_control(&mut self, val: u8) {
		// we only support setting the DLAB bit
		let val = val & 0b1000_0000;
		self.line_control_reg &= 0b0111_1111;
		self.line_control_reg |= val;
	}

	fn read_line_status(&mut self) -> u8 {
		let has_data = u8::from(self.data_queue.len() > 0);

		let tx_ready = u8::from(true);
		let tx_line_ready = u8::from(true);

		tx_line_ready << 6 | tx_ready << 5 | has_data
	}
}

bitflags::bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	struct UartInterruptKind: u8 {
		const RX_DATA_AVAILABLE = 1 << 0;
		const TX_REG_EMPTY = 1 << 1;
		const RX_STATUS_CHANGE = 1 << 2;
		const MODEM_STATUS_CHANGE = 1 << 3;
	}
}
