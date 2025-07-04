use std::convert::TryInto;
use std::mem::size_of;
use std::str;
use std::time::SystemTime;

use bytes::{Buf, BufMut, Bytes};
use gel_errors::{ClientEncodingError, Error, ErrorKind};
use snafu::{ensure, ResultExt};

use crate::codec;
use crate::descriptors::{Descriptor, TypePos};
use crate::errors::{self, DecodeError};
use crate::model::{range, Vector, VectorRef};
use crate::model::{BigInt, Decimal};
use crate::model::{ConfigMemory, Range};
use crate::model::{DateDuration, RelativeDuration};
use crate::model::{Datetime, Duration, LocalDate, LocalDatetime, LocalTime};
use crate::model::{Json, Uuid};
use crate::query_arg::{DescriptorContext, Encoder, ScalarArg};
use crate::serialization::decode::queryable::scalars::DecodeScalar;
use crate::value::{EnumValue, Value};

pub trait RawCodec<'t>: Sized {
    fn decode(buf: &'t [u8]) -> Result<Self, DecodeError>;
}

fn ensure_exact_size(buf: &[u8], expected_size: usize) -> Result<(), DecodeError> {
    if buf.len() != expected_size {
        if buf.len() < expected_size {
            return errors::Underflow.fail();
        } else {
            return errors::ExtraData.fail();
        }
    }
    Ok(())
}

impl RawCodec<'_> for String {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        <&str>::decode(buf).map(|s| s.to_owned())
    }
}

fn check_scalar(
    ctx: &DescriptorContext,
    type_pos: TypePos,
    type_id: Uuid,
    name: &str,
) -> Result<(), Error> {
    use crate::descriptors::Descriptor::{BaseScalar, Scalar};
    let desc = ctx.get(type_pos)?;
    match desc {
        Scalar(scalar) if scalar.base_type_pos.is_some() => {
            return check_scalar(ctx, scalar.base_type_pos.unwrap(), type_id, name);
        }
        Scalar(scalar) if ctx.proto.is_2() && *scalar.id == type_id => {
            return Ok(());
        }
        BaseScalar(base) if *base.id == type_id => {
            return Ok(());
        }
        _ => {}
    }
    Err(ctx.wrong_type(desc, name))
}

impl ScalarArg for String {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.extend(self.as_bytes());
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Str(self.clone()))
    }
}

impl ScalarArg for &'_ str {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.extend(self.as_bytes());
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        // special case: &str can express an enum variant
        if let Descriptor::Enumeration(_) = ctx.get(pos)? {
            return Ok(());
        }

        check_scalar(ctx, pos, String::uuid(), String::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Str(self.to_string()))
    }
}

impl<'t> RawCodec<'t> for &'t str {
    fn decode(buf: &'t [u8]) -> Result<Self, DecodeError> {
        let val = str::from_utf8(buf).context(errors::InvalidUtf8)?;
        Ok(val)
    }
}

impl ScalarArg for Json {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(self.len() + 1);
        encoder.buf.put_u8(1);
        encoder.buf.extend(self.as_bytes());
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Json::uuid(), Json::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Json(self.clone()))
    }
}

impl RawCodec<'_> for Json {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure!(buf.remaining() >= 1, errors::Underflow);
        let format = buf.get_u8();
        ensure!(format == 1, errors::InvalidJsonFormat);
        let val = str::from_utf8(buf).context(errors::InvalidUtf8)?.to_owned();
        Ok(Json::new_unchecked(val))
    }
}

impl RawCodec<'_> for Uuid {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, 16)?;
        let uuid = Uuid::from_slice(buf).unwrap();
        Ok(uuid)
    }
}

impl ScalarArg for Uuid {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(16);
        encoder.buf.extend(self.as_bytes());
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Uuid(*self))
    }
}

impl RawCodec<'_> for bool {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, 1)?;
        let res = match buf[0] {
            0x00 => false,
            0x01 => true,
            v => errors::InvalidBool { val: v }.fail()?,
        };
        Ok(res)
    }
}

impl ScalarArg for bool {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(1);
        encoder.buf.put_u8(match self {
            false => 0x00,
            true => 0x01,
        });
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Bool(*self))
    }
}

impl RawCodec<'_> for i16 {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, size_of::<Self>())?;
        Ok(buf.get_i16())
    }
}

impl ScalarArg for i16 {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(2);
        encoder.buf.put_i16(*self);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Int16(*self))
    }
}

impl RawCodec<'_> for i32 {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, size_of::<Self>())?;
        Ok(buf.get_i32())
    }
}

impl ScalarArg for i32 {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(4);
        encoder.buf.put_i32(*self);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Int32(*self))
    }
}

impl RawCodec<'_> for i64 {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, size_of::<Self>())?;
        Ok(buf.get_i64())
    }
}

impl RawCodec<'_> for ConfigMemory {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, size_of::<Self>())?;
        Ok(ConfigMemory(buf.get_i64()))
    }
}

impl ScalarArg for i64 {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(8);
        encoder.buf.put_i64(*self);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Int64(*self))
    }
}

impl RawCodec<'_> for f32 {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, size_of::<Self>())?;
        Ok(buf.get_f32())
    }
}

impl ScalarArg for f32 {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(4);
        encoder.buf.put_f32(*self);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Float32(*self))
    }
}

impl RawCodec<'_> for f64 {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, size_of::<Self>())?;
        Ok(buf.get_f64())
    }
}

impl ScalarArg for f64 {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(8);
        encoder.buf.put_f64(*self);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Float64(*self))
    }
}

impl<'t> RawCodec<'t> for &'t [u8] {
    fn decode(buf: &'t [u8]) -> Result<Self, DecodeError> {
        Ok(buf)
    }
}

impl ScalarArg for &'_ [u8] {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.extend(*self);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, codec::STD_BYTES, "std::bytes")
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Bytes(Bytes::copy_from_slice(self)))
    }
}

impl RawCodec<'_> for Bytes {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        Ok(Bytes::copy_from_slice(buf))
    }
}

impl ScalarArg for Bytes {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.extend(&self[..]);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, codec::STD_BYTES, "std::bytes")
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Bytes(self.clone()))
    }
}

impl ScalarArg for ConfigMemory {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.reserve(8);
        encoder.buf.put_i64(self.0);
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::ConfigMemory(*self))
    }
}

impl RawCodec<'_> for Decimal {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure!(buf.remaining() >= 8, errors::Underflow);
        let ndigits = buf.get_u16() as usize;
        let weight = buf.get_i16();
        let negative = match buf.get_u16() {
            0x0000 => false,
            0x4000 => true,
            _ => errors::BadSign.fail()?,
        };
        let decimal_digits = buf.get_u16();
        ensure_exact_size(buf, ndigits * 2)?;
        let mut digits = Vec::with_capacity(ndigits);
        for _ in 0..ndigits {
            digits.push(buf.get_u16());
        }
        Ok(Decimal {
            negative,
            weight,
            decimal_digits,
            digits,
        })
    }
}

#[cfg(feature = "bigdecimal")]
impl RawCodec<'_> for bigdecimal::BigDecimal {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let dec: Decimal = RawCodec::decode(buf)?;
        Ok(dec.into())
    }
}

impl ScalarArg for Decimal {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_decimal(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Decimal(self.clone()))
    }
}

#[cfg(feature = "num-bigint")]
impl RawCodec<'_> for num_bigint::BigInt {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let dec: BigInt = RawCodec::decode(buf)?;
        Ok(dec.into())
    }
}

#[cfg(feature = "bigdecimal")]
impl ScalarArg for bigdecimal::BigDecimal {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        let val = self.clone().try_into().map_err(|e| {
            ClientEncodingError::with_source(e).context("cannot serialize BigDecimal value")
        })?;
        codec::encode_decimal(encoder.buf, &val).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Decimal(
            self.clone()
                .try_into()
                .map_err(ClientEncodingError::with_source)?,
        ))
    }
}

impl RawCodec<'_> for BigInt {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure!(buf.remaining() >= 8, errors::Underflow);
        let ndigits = buf.get_u16() as usize;
        let weight = buf.get_i16();
        let negative = match buf.get_u16() {
            0x0000 => false,
            0x4000 => true,
            _ => errors::BadSign.fail()?,
        };
        let decimal_digits = buf.get_u16();
        ensure!(decimal_digits == 0, errors::NonZeroReservedBytes);
        let mut digits = Vec::with_capacity(ndigits);
        ensure_exact_size(buf, ndigits * 2)?;
        for _ in 0..ndigits {
            digits.push(buf.get_u16());
        }
        Ok(BigInt {
            negative,
            weight,
            digits,
        })
    }
}

impl ScalarArg for BigInt {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_big_int(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::BigInt(self.clone()))
    }
}

#[cfg(feature = "bigdecimal")]
impl ScalarArg for num_bigint::BigInt {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        let val = self.clone().try_into().map_err(|e| {
            ClientEncodingError::with_source(e).context("cannot serialize BigInt value")
        })?;
        codec::encode_big_int(encoder.buf, &val).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        let val = self.clone().try_into().map_err(|e| {
            ClientEncodingError::with_source(e).context("cannot serialize BigInt value")
        })?;
        Ok(Value::BigInt(val))
    }
}

impl RawCodec<'_> for Duration {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, 16)?;
        let micros = buf.get_i64();
        let days = buf.get_u32();
        let months = buf.get_u32();
        ensure!(months == 0 && days == 0, errors::NonZeroReservedBytes);
        Ok(Duration { micros })
    }
}

impl RawCodec<'_> for std::time::Duration {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let dur = Duration::decode(buf)?;
        dur.try_into().map_err(|_| errors::InvalidDate.build())
    }
}

impl ScalarArg for Duration {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_duration(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Duration(*self))
    }
}

impl RawCodec<'_> for RelativeDuration {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, 16)?;
        let micros = buf.get_i64();
        let days = buf.get_i32();
        let months = buf.get_i32();
        Ok(RelativeDuration {
            micros,
            days,
            months,
        })
    }
}

impl RawCodec<'_> for DateDuration {
    fn decode(mut buf: &[u8]) -> Result<Self, DecodeError> {
        ensure_exact_size(buf, 16)?;
        let micros = buf.get_i64();
        let days = buf.get_i32();
        let months = buf.get_i32();
        ensure!(micros == 0, errors::NonZeroReservedBytes);
        Ok(DateDuration { days, months })
    }
}

impl ScalarArg for RelativeDuration {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_relative_duration(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::RelativeDuration(*self))
    }
}

impl RawCodec<'_> for SystemTime {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let dur = Datetime::decode(buf)?;
        dur.try_into().map_err(|_| errors::InvalidDate.build())
    }
}

impl ScalarArg for SystemTime {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        let val = (*self).try_into().map_err(|e| {
            ClientEncodingError::with_source(e).context("cannot serialize SystemTime value")
        })?;
        codec::encode_datetime(encoder.buf, &val).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        let val = (*self).try_into().map_err(|e| {
            ClientEncodingError::with_source(e).context("cannot serialize SystemTime value")
        })?;
        Ok(Value::Datetime(val))
    }
}

impl RawCodec<'_> for Datetime {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let micros = i64::decode(buf)?;
        Datetime::from_postgres_micros(micros).map_err(|_| errors::InvalidDate.build())
    }
}

impl ScalarArg for Datetime {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_datetime(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Datetime(*self))
    }
}

impl RawCodec<'_> for LocalDatetime {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let micros = i64::decode(buf)?;
        LocalDatetime::from_postgres_micros(micros).map_err(|_| errors::InvalidDate.build())
    }
}

impl ScalarArg for LocalDatetime {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_local_datetime(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::LocalDatetime(*self))
    }
}

impl RawCodec<'_> for LocalDate {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let days = i32::decode(buf)?;
        Ok(LocalDate { days })
    }
}

impl ScalarArg for LocalDate {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_local_date(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::LocalDate(*self))
    }
}

impl RawCodec<'_> for LocalTime {
    fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        let micros = i64::decode(buf)?;
        ensure!(
            (0..86_400 * 1_000_000).contains(&micros),
            errors::InvalidDate
        );
        Ok(LocalTime {
            micros: micros as u64,
        })
    }
}

impl ScalarArg for DateDuration {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_date_duration(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::DateDuration(*self))
    }
}

impl ScalarArg for LocalTime {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        codec::encode_local_time(encoder.buf, self).map_err(ClientEncodingError::with_source)
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        check_scalar(ctx, pos, Self::uuid(), Self::typename())
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::LocalTime(*self))
    }
}

impl ScalarArg for EnumValue {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        encoder.buf.extend(self.as_bytes());
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        use crate::descriptors::Descriptor::Enumeration;

        let desc = ctx.get(pos)?;
        if let Enumeration(_) = desc {
            // Should we check enum members?
            // Should we override `QueryArg` check descriptor for that?
            // Or maybe implement just `QueryArg` for enum?
        }
        Err(ctx.wrong_type(desc, "enum"))
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Enum(self.clone()))
    }
}

impl<T: ScalarArg + Clone> ScalarArg for Range<T> {
    fn encode(&self, encoder: &mut Encoder) -> Result<(), Error> {
        let flags = if self.empty {
            range::EMPTY
        } else {
            (if self.inc_lower { range::LB_INC } else { 0 })
                | (if self.inc_upper { range::UB_INC } else { 0 })
                | (if self.lower.is_none() {
                    range::LB_INF
                } else {
                    0
                })
                | (if self.upper.is_none() {
                    range::UB_INF
                } else {
                    0
                })
        };
        encoder.buf.reserve(1);
        encoder.buf.put_u8(flags as u8);

        if let Some(lower) = &self.lower {
            encoder.length_prefixed(|encoder| lower.encode(encoder))?
        }

        if let Some(upper) = &self.upper {
            encoder.length_prefixed(|encoder| upper.encode(encoder))?;
        }
        Ok(())
    }
    fn check_descriptor(ctx: &DescriptorContext, pos: TypePos) -> Result<(), Error> {
        let desc = ctx.get(pos)?;
        if let Descriptor::Range(rng) = desc {
            T::check_descriptor(ctx, rng.type_pos)
        } else {
            Err(ctx.wrong_type(desc, "range"))
        }
    }
    fn to_value(&self) -> Result<Value, Error> {
        Ok(Value::Range(Range {
            lower: self
                .lower
                .as_ref()
                .map(|v| v.to_value().map(Box::new))
                .transpose()?,
            upper: self
                .upper
                .as_ref()
                .map(|v| v.to_value().map(Box::new))
                .transpose()?,
            inc_lower: self.inc_lower,
            inc_upper: self.inc_upper,
            empty: self.empty,
        }))
    }
}

impl ScalarArg for VectorRef<'_> {
    fn encode(&self, encoder: &mut crate::query_arg::Encoder) -> Result<(), gel_errors::Error> {
        encoder.buf.reserve(2 + 2 + self.0.len() * 4);
        encoder.buf.put_u16(self.0.len() as u16); // len
        encoder.buf.put_u16(0); // reserved
        for v in self.0 {
            encoder.buf.put_u32(v.to_bits());
        }
        Ok(())
    }

    fn check_descriptor(ctx: &DescriptorContext, type_pos: TypePos) -> Result<(), Error> {
        check_scalar(
            ctx,
            type_pos,
            codec::PGVECTOR_VECTOR,
            "ext::pgvector::vector",
        )
    }

    fn to_value(&self) -> Result<Value, gel_errors::Error> {
        Ok(Value::Vector(self.0.to_vec()))
    }
}

impl ScalarArg for Vector {
    fn encode(&self, encoder: &mut crate::query_arg::Encoder) -> Result<(), gel_errors::Error> {
        VectorRef(&self.0).encode(encoder)
    }

    fn check_descriptor(ctx: &DescriptorContext, type_pos: TypePos) -> Result<(), Error> {
        VectorRef::check_descriptor(ctx, type_pos)
    }

    fn to_value(&self) -> Result<Value, gel_errors::Error> {
        VectorRef(&self.0).to_value()
    }
}
