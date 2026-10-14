//! The photoresistor: a voltage divider into an ADC pin. More light, less
//! resistance, higher voltage.
//!
//! This talks to ESP-IDF's oneshot ADC driver directly: esp-idf-hal 0.46 has
//! the ESP32-C5's ADC channels off by one (it puts GPIO1 on channel 1; it's
//! channel 0), so we let ESP-IDF find the channel for the pin.

use esp_idf_svc::hal::adc::ADC1;
use esp_idf_svc::hal::gpio::Pin;
use esp_idf_svc::sys::{
    adc_atten_t_ADC_ATTEN_DB_12, adc_bitwidth_t_ADC_BITWIDTH_12, adc_channel_t,
    adc_oneshot_chan_cfg_t, adc_oneshot_config_channel, adc_oneshot_del_unit,
    adc_oneshot_io_to_channel, adc_oneshot_new_unit, adc_oneshot_read,
    adc_oneshot_unit_handle_t, adc_oneshot_unit_init_cfg_t, adc_unit_t, esp, EspError,
};

/// The largest 12-bit reading.
const FULL_SCALE: f32 = 4095.0;

pub struct LightSensor {
    unit: adc_oneshot_unit_handle_t,
    channel: adc_channel_t,
    /// A running average, so a flicker doesn't switch the lamp.
    smoothed: Option<f32>,
}

impl LightSensor {
    /// Takes ADC1 so nothing else configures it.
    pub fn new(_adc: ADC1<'static>, pin: impl Pin + 'static) -> Result<Self, EspError> {
        let mut unit_id: adc_unit_t = 0;
        let mut channel: adc_channel_t = 0;
        esp!(unsafe { adc_oneshot_io_to_channel(pin.pin().into(), &mut unit_id, &mut channel) })?;
        let mut unit = std::ptr::null_mut();
        let unit_config = adc_oneshot_unit_init_cfg_t {
            unit_id,
            ..Default::default()
        };
        esp!(unsafe { adc_oneshot_new_unit(&unit_config, &mut unit) })?;
        // 12 dB attenuation: the full 0 to 3.3 V range.
        let channel_config = adc_oneshot_chan_cfg_t {
            atten: adc_atten_t_ADC_ATTEN_DB_12,
            bitwidth: adc_bitwidth_t_ADC_BITWIDTH_12,
        };
        esp!(unsafe { adc_oneshot_config_channel(unit, channel, &channel_config) })?;
        Ok(Self {
            unit,
            channel,
            smoothed: None,
        })
    }

    /// Take a reading, and return the smoothed brightness: 0 (dark) to 100
    /// (bright), as a fraction so small changes still count.
    pub fn read(&mut self) -> Result<f32, EspError> {
        let mut raw = 0;
        esp!(unsafe { adc_oneshot_read(self.unit, self.channel, &mut raw) })?;
        let level = raw as f32 * 100.0 / FULL_SCALE;
        // An exponential moving average: each reading counts 1/16. At one
        // reading every 50 ms, that settles in about a second.
        let smoothed = match self.smoothed {
            Some(previous) => previous + (level - previous) / 16.0,
            None => level,
        };
        self.smoothed = Some(smoothed);
        Ok(smoothed)
    }
}

impl Drop for LightSensor {
    fn drop(&mut self) {
        unsafe { adc_oneshot_del_unit(self.unit) };
    }
}
