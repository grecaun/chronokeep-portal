#[cfg(target_os = "linux")]
use i2c_character_display::{AdafruitLCDBackpack, LcdDisplayType, BaseCharacterDisplay};
#[cfg(target_os = "linux")]
use rppal::{hal, I2c};

#[derive(Clone)]
pub struct CharacterDisplay {
    #[cfg(target_os = "linux")]
    pub lcd: BaseCharacterDisplay
}

impl CharacterDisplay {
    pub fn new(bus: i32) -> Self {
        println!("Attempting to connect to screen on i2c{bus}.");
        #[cfg(target_os = "linux")]
        {
            let i2c = I2c::with_bus(bus).unwrap();
            let delay = hal::Delay::new();

            let mut lcd = AdafruitLCDBackpack::new(i2c, LcdDisplayType::Lcd20x4, delay);
            lcd.backlight(true)?.clear()?.home()?;
            lcd.print("12345678901234567890\n12345678901234567890\n12345678901234567890\n12345678901234567890")?;
        }
        Self {
            #[cfg(target_os = "linux")]
            lcd
        }
    }

    pub fn print(_msg: &str) {
        #[cfg(target_os = "linux")]
        lcd.print(_msg)?;
    }
}