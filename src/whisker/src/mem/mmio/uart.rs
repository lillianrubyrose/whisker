use std::{
	collections::VecDeque,
	io::{Read, Write},
	process::{Command, Stdio},
	sync::{Arc, mpsc::Sender},
	thread,
};

use bitfield::prelude::*;
use num_conv::Truncate;
use socketpair::socketpair_stream;
use spin::Mutex;

use crate::{
	cpu::hart::WhiskerHart,
	interrupts::{InterruptMessage, InterruptSource},
	mem::mmio::MMIODevice,
	tracing::*,
	util,
};

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

bitflags::bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	struct LineStatus: u8 {
		const DATA_READY = 1 << 0;
		const OVERRUN_ERROR = 1 << 1;
		const PARITY_ERROR = 1 << 2;
		const FRAMING_ERROR = 1 << 3;
		const BREAK_INTERRUPT = 1 << 4;
		const TX_HOLDING_REG_EMPTY = 1 << 5;
		const TX_EMPTY = 1 << 6;
		const FIFO_ERROR = 1 << 7;
	}
}

impl Default for LineStatus {
	fn default() -> Self {
		// Transmitter is always ready to accept new characters.
		Self::TX_HOLDING_REG_EMPTY | Self::TX_EMPTY
	}
}

#[bitfields]
#[derive(Debug, Clone, Copy)]
struct LineControl {
	word_length: U2,
	stop_bits: bool,
	parity_enable: bool,
	even_parity: bool,
	stick_parity: bool,
	break_control: bool,
	dlab: bool,
}

#[bitfields]
#[derive(Debug, Clone, Copy, Default)]
struct ModemControl {
	data_term_ready: bool,
	request_to_send: bool,
	out1: bool,
	interrupt_enable: bool,
	loopback: bool,
	_res: U3,
}

#[bitfields]
#[derive(Debug, Clone, Copy)]
struct ModemStatus {
	delta_clear_to_send: bool,
	delta_data_set_ready: bool,
	trailing_edge_ring_indicator: bool,
	delta_data_carrier_detect: bool,
	clear_to_send: bool,
	data_set_ready: bool,
	ring_indicator: bool,
	data_carrier_detect: bool,
}

impl Default for ModemStatus {
	fn default() -> Self {
		let mut s = Self::new();
		s.set_data_set_ready(true);
		s.set_clear_to_send(true);
		s
	}
}

pub struct UART {
	interrupt_enable: UartInterruptKind,
	queue_interrupt_level: u8,

	line_control: LineControl,
	line_status: LineStatus,
	modem_control: ModemControl,
	modem_status: ModemStatus,

	scratch_reg: u8,

	divisor: u16,

	interrupt_id: InterruptIdentification,

	data_queue: VecDeque<u8>,

	stdout: Box<dyn Write + Send + Sync>,
	interrupt_tx: Sender<InterruptMessage>,
}

#[cfg(target_family = "unix")]
fn spawn_io_term() -> Result<(impl Read + Send, impl Write + Send + Sync), String> {
	use std::os::fd::{AsFd, RawFd};

	use command_fds::{CommandFdExt as _, FdMapping};

	const REMOTE_FD_NUM: RawFd = 4;
	let (local, other) = socketpair_stream().map_err(|_| "unable to create socket pair")?;

	let term = util::find_terminal().map_err(|_| "unable to find a terminal")?;
	let mut cmd = Command::new(&term);
	cmd.args([
		"-e",
		"socat",
		"stdio,raw,echo=0,icrnl,opost",
		format!("FD:{REMOTE_FD_NUM},crnl").as_str(),
	])
	.stdin(Stdio::null())
	.stdout(Stdio::null())
	.stderr(Stdio::null())
	.fd_mappings(vec![FdMapping {
		parent_fd: other
			.as_fd()
			.try_clone_to_owned()
			.map_err(|_| "unable to clone fd for uart")?,
		child_fd: REMOTE_FD_NUM,
	}])
	.map_err(|_| "fd collided in UART terminal")?;
	// FIXME: It closes on CTRL-C but doesn't close on panic
	// This is intended to live forever.
	#[allow(clippy::zombie_processes)]
	let _ = cmd
		.spawn()
		.map_err(|e| format!("unable to spawn UART terminal `{}`: {:?}", term, e))?;

	let reader = local.try_clone().map_err(|_| "could not clone socket")?;
	let writer = local;
	Ok((reader, writer))
}

#[cfg(target_family = "windows")]
fn spawn_io_term() -> Result<(impl Read + Send, impl Write + Send + Sync), String> {
	use std::io::{stdin, stdout};
	Ok((stdin(), stdout()))
}

impl UART {
	pub fn init(interrupt_tx: Sender<InterruptMessage>) -> Arc<Mutex<Self>> {
		#[cfg(not(test))]
		let (mut reader, writer) = spawn_io_term().expect("failed to initialize UART");
		#[cfg(test)]
		let (mut reader, writer) = (std::io::stdin(), std::io::stdout());

		let mut line_control = LineControl::new();
		line_control.set_word_length(3); // 8 bits

		let this = Arc::new(Mutex::new(Self {
			interrupt_enable: UartInterruptKind::empty(),
			queue_interrupt_level: 1,

			line_control,
			line_status: LineStatus::default(),
			modem_control: ModemControl::default(),
			modem_status: ModemStatus::default(),

			scratch_reg: 0,

			divisor: 1,

			interrupt_id: InterruptIdentification::new(),

			data_queue: VecDeque::new(),
			stdout: Box::new(writer),
			interrupt_tx,
		}));

		thread::spawn({
			let uart = Arc::clone(&this);
			move || 'outer: loop {
				let mut b = 0_u8;
				let Ok(()) = reader.read_exact(core::slice::from_mut(&mut b)) else {
					error!("reading from UART term failed");
					break 'outer;
				};

				trace!("UART read: {:0X}", b);

				let mut uart = uart.lock();
				uart.data_queue.push_back(b);
				uart.line_status.insert(LineStatus::DATA_READY);
				if uart.interrupt_enable.contains(UartInterruptKind::RX_DATA_AVAILABLE)
					&& uart.data_queue.len() >= usize::from(uart.queue_interrupt_level)
				{
					uart.interrupt_id.set_pending(true);
					uart.interrupt_id.set_kind(InterruptReason::ReceivedDataAvailable);
					uart.interrupt_tx
						.send(InterruptMessage::new_high(InterruptSource::UART))
						.expect("could not send to interrupt controller");
				}
			}
		});

		this
	}
}

impl MMIODevice for UART {
	fn read(&mut self, _: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		let out = &mut buf[0];
		let is_dlab = self.line_control.get_dlab();
		match addr {
			DATA_REG => {
				if is_dlab {
					// divisor LSB
					*out = (self.divisor & 0xFF).truncate();
				} else {
					*out = self.do_read();
				}
			}
			INTERRUPT_ENABLE_REG => {
				if is_dlab {
					// divisor MSB
					*out = (self.divisor >> 8).truncate();
				} else {
					*out = self.interrupt_enable.bits();
				}
			}
			INTERRUPT_IDENT_REG => {
				let ret = u8::from_le_bytes(self.interrupt_id.inner());
				if self.interrupt_id.get_kind() == InterruptReason::TransmitterHoldingRegisterEmpty {
					self.interrupt_id.set_pending(false);
				}
				*out = ret;
			}
			LINE_CONTROL_REG => *out = u8::from_le_bytes(self.line_control.inner()),
			MODEM_CONTROL_REG => *out = u8::from_le_bytes(self.modem_control.inner()),
			LINE_STATUS_REG => *out = self.line_status.bits(),
			MODEM_STATUS_REG => *out = u8::from_le_bytes(self.modem_status.inner()),
			SCRATCH_REG => *out = self.scratch_reg,
			_ => warn!("read from unknown UART addr {:#018X}", addr),
		}
	}

	fn write(&mut self, _: &mut WhiskerHart, addr: u64, val: &[u8]) {
		let val = val[0];
		let is_dlab = self.line_control.get_dlab();
		match addr {
			DATA_REG => {
				if is_dlab {
					// divisor LSB
					self.divisor &= 0xFF00;
					self.divisor |= u16::from(val);
				} else {
					self.do_write(val);
				}
			}
			INTERRUPT_ENABLE_REG => {
				if is_dlab {
					//divisor MSB
					self.divisor &= 0x00FF;
					self.divisor |= u16::from(val) << 8;
				} else {
					self.interrupt_enable = UartInterruptKind::from_bits_retain(val);
				}
			}
			FIFO_CONTROL_REG => self.write_fifo_control(val),
			LINE_CONTROL_REG => self.line_control.set_inner(val.to_le_bytes()),
			MODEM_CONTROL_REG => self.modem_control.set_inner(val.to_le_bytes()),
			LINE_STATUS_REG => {} // ignored
			MODEM_STATUS_REG => {
				warn!("ignored write of {:02X} to UART Modem Status Register", val);
			}
			SCRATCH_REG => self.scratch_reg = val,
			_ => {
				warn!("write to unknown UART addr {:#018X}", addr);
			}
		}
	}
}

impl UART {
	fn do_read(&mut self) -> u8 {
		if let Some(val) = self.data_queue.pop_front() {
			if self.data_queue.is_empty() {
				self.line_status.remove(LineStatus::DATA_READY);
			}
			if self.data_queue.len() < usize::from(self.queue_interrupt_level) {
				self.interrupt_id.set_pending(false);
				self.interrupt_tx
					.send(InterruptMessage::new_low(InterruptSource::UART))
					.expect("unable to send interrupt controller");
			}
			val
		} else {
			0
		}
	}

	fn do_write(&mut self, val: u8) {
		if self.modem_control.get_loopback() {
			self.data_queue.push_back(val);
			return;
		}

		let _ = write!(&mut self.stdout, "{}", val as char);
		// FIXME: it would be nice to flush stdout all the time but its VERY slow, reconsider this
		//let _ = self.stdout.flush();

		// writing to THR resets interrupt
		self.interrupt_tx
			.send(InterruptMessage::new_low(InterruptSource::UART))
			.unwrap();

		// the transmitter register is considered to immedately be empty
		if self.interrupt_enable.contains(UartInterruptKind::TX_REG_EMPTY) {
			// FIXME: consider limiting this?
			// 8N1 @ 115200 baud is 11520 bytes per second
			// it might be a good idea to hold off on sending interrupts to allow
			// the cpu to process other things
			self.interrupt_id.set_pending(true);
			self.interrupt_id
				.set_kind(InterruptReason::TransmitterHoldingRegisterEmpty);
			self.interrupt_tx
				.send(InterruptMessage::new_high(InterruptSource::UART))
				.expect("could not send to interrupt controller");
		}
	}

	fn write_fifo_control(&mut self, val: u8) {
		if val & 1 != 0 {
			self.interrupt_id.set_fifo_enabled(0b11);
		} else {
			self.interrupt_id.set_fifo_enabled(0);
		}
		if val & (1 << 1) != 0 {
			self.data_queue.clear();
		}
		if val & (1 << 2) != 0 {
			// clear transmit queue
		}
		if val & (1 << 3) != 0 {
			error!("DMA mode not supported");
		}
		let level = val >> 6;
		let queue_size = match level {
			0 => 1,
			1 => 4,
			2 => 8,
			3 => 14,
			_ => unreachable!(),
		};
		self.queue_interrupt_level = queue_size;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, BitFieldRepr)]
enum InterruptReason {
	ModemStatus = 0b000,
	TransmitterHoldingRegisterEmpty = 0b001,
	ReceivedDataAvailable = 0b010,
	ReceiverLineStatus = 0b011,
	CharacterTimeout = 0b110,
}

impl InterruptReason {
	fn bits(self) -> u8 {
		self as u8
	}
}

#[bitfields]
#[derive(Debug, Clone, Copy)]
struct InterruptIdentification {
	pending: bool,
	kind: InterruptReason,
	_res_4_5: U2,
	fifo_enabled: U2,
}
