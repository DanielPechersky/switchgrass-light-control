use core::num::Wrapping;

use esp_hal::{
    Async,
    uart::{self, UartRx},
};

pub type LedValues = [u8; 150];

#[derive(defmt::Format)]
pub enum ReadMessageError {
    Uart(uart::RxError),
    ChecksumMismatch,
}

pub async fn read_message(
    uart: &mut UartRx<'static, Async>,
) -> Result<LedValues, ReadMessageError> {
    read_until_start_of_message(uart)
        .await
        .map_err(ReadMessageError::Uart)?;

    let mut leds: LedValues = [0; 150];
    uart.read_exact_async(&mut leds)
        .await
        .map_err(ReadMessageError::Uart)?;

    let mut read_checksum = 0;
    uart.read_exact_async(core::array::from_mut(&mut read_checksum))
        .await
        .map_err(ReadMessageError::Uart)?;

    let calculated_checksum = leds.iter().copied().map(Wrapping).sum::<Wrapping<u8>>().0;
    if calculated_checksum != read_checksum {
        return Err(ReadMessageError::ChecksumMismatch);
    }

    Ok(leds)
}

const MESSAGE_MAGIC_BYTES: [u8; 4] = *b"SGLP";

async fn read_until_start_of_message(
    uart: &mut UartRx<'static, Async>,
) -> Result<(), uart::RxError> {
    loop {
        let mut start_of_message = [0u8; 4];
        let (first_byte, rest_bytes) = start_of_message.split_first_mut().unwrap();

        while *first_byte != MESSAGE_MAGIC_BYTES[0] {
            uart.read_exact_async(core::array::from_mut(first_byte))
                .await?;
        }
        uart.read_exact_async(rest_bytes).await?;

        if start_of_message == MESSAGE_MAGIC_BYTES {
            return Ok(());
        }
    }
}
