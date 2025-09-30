use std::{
	sync::mpsc::Sender,
	time::{SystemTime, UNIX_EPOCH},
};

use crate::{
	cpu::hart::WhiskerHart,
	interrupts::{InterruptMessage, InterruptSource},
	mem::mmio::MMIODevice,
};

pub const GOLDFISH_RTC_BASE: u64 = 0x101000;
pub const GOLDFISH_RTC_SIZE: u64 = 0x1000;

const RTC_TIME_LOW: u64 = 0x00;
const RTC_TIME_HIGH: u64 = 0x04;
const RTC_ALARM_LOW: u64 = 0x08;
const RTC_ALARM_HIGH: u64 = 0x0c;
const RTC_IRQ_ENABLED: u64 = 0x10;
const RTC_CLEAR_ALARM: u64 = 0x14;
const RTC_ALARM_STATUS: u64 = 0x18;
const RTC_CLEAR_INTERRUPT: u64 = 0x1c;

#[derive(Debug)]
pub struct GoldfishRTC {
	start_time_ns: u64,
	alarm_time_ns: u64,
	irq_enabled: bool,
	alarm_pending: bool,
	interrupt_tx: Sender<InterruptMessage>,
}

impl GoldfishRTC {
	pub const WHISKER_EPOCH_NS: u64 = 1739440200000;

	pub fn new(interrupt_tx: Sender<InterruptMessage>) -> Self {
		Self {
			start_time_ns: Self::WHISKER_EPOCH_NS
				.saturating_sub(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64),
			alarm_time_ns: 0,
			irq_enabled: false,
			alarm_pending: false,
			interrupt_tx,
		}
	}

	fn get_current_time_ns(&self) -> u64 {
		self.start_time_ns
			.wrapping_add(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64)
	}

	pub fn step(&mut self) {
		if !self.irq_enabled || self.alarm_pending {
			return;
		}

		let current_time = self.get_current_time_ns();
		if self.alarm_time_ns > 0 && current_time >= self.alarm_time_ns {
			self.alarm_pending = true;
			self.interrupt_tx
				.send(InterruptMessage::new_high(InterruptSource::RTC))
				.expect("could not send to interrupt controller");
		}
	}
}

impl MMIODevice for GoldfishRTC {
	fn read(&mut self, _hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		let current_time = self.get_current_time_ns();
		let val = match addr {
			RTC_TIME_LOW => current_time as u32,
			RTC_TIME_HIGH => (current_time >> 32) as u32,
			RTC_ALARM_LOW => self.alarm_time_ns as u32,
			RTC_ALARM_HIGH => (self.alarm_time_ns >> 32) as u32,
			RTC_IRQ_ENABLED => self.irq_enabled as u32,
			RTC_ALARM_STATUS => self.alarm_pending as u32,
			_ => 0,
		};

		buf.copy_from_slice(&val.to_le_bytes());
	}

	fn write(&mut self, _hart: &mut WhiskerHart, addr: u64, val: &[u8]) {
		let mut bytes = [0u8; 4];
		bytes.copy_from_slice(val);

		let val = u32::from_le_bytes(bytes);
		match addr {
			RTC_ALARM_LOW => {
				self.alarm_time_ns = (self.alarm_time_ns & 0xFFFFFFFF_00000000) | (val as u64);
			}
			RTC_ALARM_HIGH => {
				self.alarm_time_ns = (self.alarm_time_ns & 0x00000000_FFFFFFFF) | ((val as u64) << 32);
			}
			RTC_IRQ_ENABLED => {
				self.irq_enabled = val != 0;
				if !self.irq_enabled && self.alarm_pending {
					self.alarm_pending = false;
					self.interrupt_tx
						.send(InterruptMessage::new_low(InterruptSource::RTC))
						.expect("could not send to interrupt controller");
				}
			}
			RTC_CLEAR_ALARM | RTC_CLEAR_INTERRUPT => {
				if self.alarm_pending {
					self.alarm_pending = false;
					self.interrupt_tx
						.send(InterruptMessage::new_low(InterruptSource::RTC))
						.expect("could not send to interrupt controller");
				}
			}
			RTC_TIME_LOW | RTC_TIME_HIGH | RTC_ALARM_STATUS | _ => {}
		}
	}
}
