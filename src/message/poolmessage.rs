// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkOS library.

// The snarkOS library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkOS library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkOS library. If not, see <https://www.gnu.org/licenses/>.

use snarkvm::prelude::*;

use ::bytes::{Buf, BufMut, BytesMut};
use anyhow::{anyhow, Result};
use std::{default::Default, io::Write};
use tokio_util::codec::{Decoder, Encoder};

use ::bytes::Bytes;
use tokio::task;

const MAXIMUM_MESSAGE_SIZE: usize = 512;

/// This object enables deferred deserialization / ahead-of-time serialization for objects that
/// take a while to deserialize / serialize, in order to allow these operations to be non-blocking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Data<T: FromBytes + ToBytes + Send + 'static> {
    Object(T),
    Buffer(Bytes),
}

impl<T: FromBytes + ToBytes + Send + 'static> Data<T> {
    pub async fn deserialize(self) -> Result<T> {
        match self {
            Self::Object(x) => Ok(x),
            Self::Buffer(bytes) => {
                match task::spawn_blocking(move || T::from_bytes_le(&bytes)).await {
                    Ok(x) => x,
                    Err(err) => Err(err.into()),
                }
            }
        }
    }

    pub fn deserialize_blocking(self) -> Result<T> {
        match self {
            Self::Object(x) => Ok(x),
            Self::Buffer(bytes) => T::from_bytes_le(&bytes),
        }
    }

    pub async fn serialize(self) -> Result<Bytes> {
        match self {
            Self::Object(x) => match task::spawn_blocking(move || x.to_bytes_le()).await {
                Ok(bytes) => bytes.map(|vec| vec.into()),
                Err(err) => Err(err.into()),
            },
            Self::Buffer(bytes) => Ok(bytes),
        }
    }

    pub fn serialize_blocking_into<W: Write>(&self, writer: &mut W) -> Result<()> {
        match self {
            Self::Object(x) => {
                let bytes = x.to_bytes_le()?;
                Ok(writer.write_all(&bytes)?)
            }
            Self::Buffer(bytes) => Ok(writer.write_all(bytes)?),
        }
    }
}

#[derive(Clone, Debug)]
pub enum PoolMessageSC<N: Network> {
    /// ConnectAck := (is_accecpt, address, [id], [signature])
    ConnectAck(bool, Address<N>, Option<u32>, Option<String>),
    /// Notify := (job_id, target, epoch_challenge)
    Notify(u64, u64, EpochChallenge<N>),
    /// ShutDown := ()
    ShutDown,
    /// Pong
    Pong,
    /// Unused
    #[allow(unused)]
    Unused,
}
impl<N: Network> Default for PoolMessageSC<N> {
    fn default() -> Self {
        Self::Unused
    }
}

impl<N: Network> PoolMessageSC<N> {
    /// Returns the messge name
    #[inline]
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        match self {
            Self::ConnectAck(..) => "ConnectAck",
            Self::Notify(..) => "Notify",
            Self::ShutDown => "Shutdown",
            Self::Pong => "Pong",
            Self::Unused => "Unused",
        }
    }

    /// Returns the message ID.
    #[inline]
    pub fn id(&self) -> u8 {
        match self {
            Self::ConnectAck(..) => 0,
            Self::Notify(..) => 1,
            Self::ShutDown => 2,
            Self::Pong => 3,
            Self::Unused => 127,
        }
    }

    /// Returns the message data as bytes.
    #[inline]
    pub fn serialize_data_into<W: Write>(&self, writer: &mut W) -> Result<()> {
        match self {
            Self::ConnectAck(is_accept, address, id, signature) => match is_accept {
                true => match (id, signature) {
                    (Some(id), Some(signature)) => {
                        writer.write_all(&[1u8])?;
                        bincode::serialize_into(&mut *writer, address)?;
                        writer.write_all(&id.to_le_bytes())?;
                        writer.write_all(signature.as_bytes())?;
                        Ok(())
                    }
                    _ => Err(anyhow!("ConnectAck: Invalid id")),
                },
                false => {
                    writer.write_all(&[0u8])?;
                    bincode::serialize_into(&mut *writer, address)?;
                    Ok(())
                }
            },
            Self::Notify(job_id, target, epoch_challenge) => {
                bincode::serialize_into(&mut *writer, job_id)?;
                bincode::serialize_into(&mut *writer, target)?;
                writer.write_all(&epoch_challenge.to_bytes_le()?)?;
                Ok(())
            }
            Self::ShutDown => Ok(()),
            Self::Pong => Ok(()),
            Self::Unused => Ok(()),
        }
    }

    /// Serializes the given message into bytes.
    #[inline]
    pub fn serialize_into<W: Write>(&self, writer: &mut W) -> Result<()> {
        bincode::serialize_into(&mut *writer, &self.id())?;
        self.serialize_data_into(writer)
    }

    /// Deserializes the given buffer into a message.
    #[inline]
    pub fn deserialize(buffer: &[u8]) -> Result<Self> {
        if buffer.is_empty() {
            return Err(anyhow!("Invalid message buffer"));
        }

        let (id, data) = (buffer[0], &buffer[1..]);

        let message = match id {
            0 => match data.is_empty() {
                true => return Err(anyhow!("Invalid message buffer")),
                false => match data[0] {
                    0 => Self::ConnectAck(false, bincode::deserialize(&data[1..=32])?, None, None),
                    1 => Self::ConnectAck(
                        true,
                        bincode::deserialize(&data[1..=32])?,
                        Some(u32::from_le_bytes([data[33], data[34], data[35], data[36]])),
                        Some(String::from_utf8(data[37..].to_vec())?),
                    ),
                    _ => {
                        return Err(anyhow!(
                            "Invalid 'ConnectAck' message: {:?} {:?}",
                            buffer,
                            data
                        ))
                    }
                },
            },
            1 => Self::Notify(
                bincode::deserialize(&data[0..8])?,
                bincode::deserialize(&data[8..16])?,
                EpochChallenge::read_le(&data[16..])?,
            ),
            2 => match data.is_empty() {
                true => Self::ShutDown,
                false => {
                    return Err(anyhow!(
                        "Invalid 'ShutDown' message: {:?} {:?}",
                        buffer,
                        data
                    ))
                }
            },
            3 => match data.is_empty() {
                true => Self::Pong,
                false => return Err(anyhow!("Invalid 'Pong' message: {:?} {:?}", buffer, data)),
            },
            _ => return Err(anyhow!("Invalid message ID {}", id)),
        };

        Ok(message)
    }
}

impl<N: Network> Encoder<PoolMessageSC<N>> for PoolMessageSC<N> {
    type Error = anyhow::Error;

    fn encode(&mut self, message: PoolMessageSC<N>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&0u32.to_le_bytes());
        message.serialize_into(&mut dst.writer())?;
        let len_slice = (dst[4..].len() as u32).to_le_bytes();
        dst[..4].copy_from_slice(&len_slice);
        Ok(())
    }
}

impl<N: Network> Decoder for PoolMessageSC<N> {
    type Error = std::io::Error;
    type Item = PoolMessageSC<N>;

    fn decode(&mut self, source: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if source.len() < 4 {
            return Ok(None);
        }
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&source[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;
        // Check that the length is not too large to avoid a denial of
        // service attack where the node server runs out of memory.
        if length > MAXIMUM_MESSAGE_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", length),
            ));
        }

        if source.len() < 4 + length {
            // The full message has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            source.reserve(4 + length - source.len());

            // We inform `Framed` that we need more bytes to form the next frame.
            return Ok(None);
        }

        // Convert the buffer to a message, or fail if it is not valid.
        let message = match PoolMessageSC::deserialize(&source[4..][..length]) {
            Ok(message) => Ok(Some(message)),
            Err(error) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        };

        // Use `advance` to modify the source such that it no longer contains this frame.
        source.advance(4 + length);

        message
    }
}
#[derive(Clone, Debug)]
pub enum PoolMessageCS<N: Network> {
    /// Connect := (type, address_type, version(major, minor, patch), name, address)
    Connect(u8, u8, u8, u8, u8, String, String),
    /// submit := (work_id, job_id, address, prover_solution)
    Submit(u32, u64, Data<ProverSolution<N>>),
    /// DisConnect := (id)
    DisConnect(u32),
    /// Ping
    Ping,
    // Unused
    #[allow(unused)]
    Unused,
}

impl<N: Network> Default for PoolMessageCS<N> {
    fn default() -> Self {
        Self::Unused
    }
}

impl<N: Network> PoolMessageCS<N> {
    /// Returns the messge name
    #[inline]
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        match self {
            Self::Connect(..) => "Connect",
            Self::Submit(..) => "Submit",
            Self::DisConnect(..) => "Disconnect",
            Self::Ping => "Ping",
            Self::Unused => "Unused",
        }
    }

    /// Returns the message ID.
    pub fn id(&self) -> u8 {
        match self {
            Self::Connect(..) => 128,
            Self::Submit(..) => 129,
            Self::DisConnect(..) => 130,
            Self::Ping => 131,
            Self::Unused => 255,
        }
    }

    /// Returns the message data as bytes.
    #[inline]
    pub fn serialize_data_into<W: Write>(&self, writer: &mut W) -> Result<()> {
        match self {
            Self::Connect(
                worker_type,
                address_type,
                v_major,
                v_minor,
                v_patch,
                custom_name,
                address,
            ) => {
                writer.write_all(&[*worker_type])?;
                writer.write_all(&[*address_type])?;
                writer.write_all(&[*v_major])?;
                writer.write_all(&[*v_minor])?;
                writer.write_all(&[*v_patch])?;
                let len = custom_name.len() as u8;
                writer.write_all(&[len])?;
                writer.write_all(custom_name.as_bytes())?;
                //bincode::serialize_into(&mut *writer, custom_name)?;
                //bincode::serialize_into(&mut *writer, address)?;
                writer.write_all(address.as_bytes())?;
                Ok(())
            }
            Self::Submit(worker_id, job_id, prover_solution) => {
                bincode::serialize_into(&mut *writer, worker_id)?;
                bincode::serialize_into(&mut *writer, job_id)?;
                prover_solution.serialize_blocking_into(writer)
            }
            Self::DisConnect(id) => {
                bincode::serialize_into(&mut *writer, id)?;
                Ok(())
            }
            Self::Ping => Ok(()),
            Self::Unused => Ok(()),
        }
    }

    /// Serializes the given message into bytes.
    #[inline]
    pub fn serialize_into<W: Write>(&self, writer: &mut W) -> Result<()> {
        bincode::serialize_into(&mut *writer, &self.id())?;
        self.serialize_data_into(writer)
    }

    /// Deserializes the given buffer into a message.
    #[inline]
    pub fn deserialize(buffer: &[u8]) -> Result<Self> {
        if buffer.is_empty() {
            return Err(anyhow!("Invalid message buffer"));
        }

        let (id, data) = (buffer[0], &buffer[1..]);

        let message = match id {
            128 => {
                let name_end = (6 + data[5]) as usize;
                Self::Connect(
                    data[0],
                    data[1],
                    data[2],
                    data[3],
                    data[4],
                    String::from_utf8((data[6..name_end]).to_vec())?,
                    String::from_utf8((data[name_end..]).to_vec())?,
                )
            }
            129 => Self::Submit(
                bincode::deserialize(&data[0..4])?,
                bincode::deserialize(&data[4..12])?,
                Data::Buffer(data[12..].to_vec().into()),
            ),
            130 => Self::DisConnect(bincode::deserialize(data)?),
            131 => match data.is_empty() {
                true => Self::Ping,
                false => return Err(anyhow!("Invalid 'Ping' message: {:?} {:?}", buffer, data)),
            },
            _ => return Err(anyhow!("Invalid message ID {}", id)),
        };

        Ok(message)
    }
}

impl<N: Network> Encoder<PoolMessageCS<N>> for PoolMessageCS<N> {
    type Error = anyhow::Error;

    fn encode(&mut self, message: PoolMessageCS<N>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&0u32.to_le_bytes());
        message.serialize_into(&mut dst.writer())?;
        let len_slice = (dst[4..].len() as u32).to_le_bytes();
        dst[..4].copy_from_slice(&len_slice);
        Ok(())
    }
}

impl<N: Network> Decoder for PoolMessageCS<N> {
    type Error = std::io::Error;
    type Item = PoolMessageCS<N>;

    fn decode(&mut self, source: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if source.len() < 4 {
            return Ok(None);
        }
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&source[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;
        // Check that the length is not too large to avoid a denial of
        // service attack where the node server runs out of memory.
        if length > MAXIMUM_MESSAGE_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", length),
            ));
        }

        if source.len() < 4 + length {
            // The full message has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            source.reserve(4 + length - source.len());

            // We inform `Framed` that we need more bytes to form the next frame.
            return Ok(None);
        }

        // Convert the buffer to a message, or fail if it is not valid.
        let message = match PoolMessageCS::deserialize(&source[4..][..length]) {
            Ok(message) => Ok(Some(message)),
            Err(error) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        };

        // Use `advance` to modify the source such that it no longer contains this frame.
        source.advance(4 + length);

        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::rand::thread_rng;
    use snarkvm::prelude::Testnet3;
    type CurrentNetwork = Testnet3;
    // use snarkvm_console_network_environment::Console;
    // type CurrentEnvironment = Console;
    use snarkvm_algorithms::polycommit::kzg10::{KZGCommitment, KZGProof};

    fn check_pool_message_sc(message: PoolMessageSC<CurrentNetwork>) {
        println!("{:?}", message);
        let mut buffer = BytesMut::new();
        let _ = PoolMessageSC::<CurrentNetwork>::default().encode(message, &mut buffer);
        println!("{:?}", buffer);
        let message1 = PoolMessageSC::<CurrentNetwork>::default()
            .decode(&mut buffer.clone())
            .unwrap()
            .unwrap();
        println!("{:?}", message1);
        let mut buffer_2 = BytesMut::new();
        let _ = PoolMessageSC::<CurrentNetwork>::default().encode(message1, &mut buffer_2);
        assert_eq!(buffer, buffer_2);
    }

    fn check_pool_message_cs(message: PoolMessageCS<CurrentNetwork>) {
        println!("message: {:?}", message);
        let mut buffer = BytesMut::new();
        let _ = PoolMessageCS::<CurrentNetwork>::default().encode(message, &mut buffer);
        println!("buffer: {:?}", buffer);
        let message1 = PoolMessageCS::<CurrentNetwork>::default()
            .decode(&mut buffer.clone())
            .unwrap()
            .unwrap();
        println!("message: {:?}", message1);
        let mut buffer_2 = BytesMut::new();
        let _ = PoolMessageCS::default().encode(message1, &mut buffer_2);
        println!("buffer: {:?}", buffer_2);
        assert_eq!(buffer, buffer_2);
    }

    #[test]
    fn test_pool_message_sc() -> Result<()> {
        // env
        let rng = &mut thread_rng();
        let address = Address::<CurrentNetwork>::new(Uniform::rand(rng));
        println!("{}", address);

        let message = PoolMessageSC::ConnectAck::<CurrentNetwork>(
            true,
            address,
            Some(1),
            Some(String::from("testsignature")),
        );
        check_pool_message_sc(message);

        let epoch_challenge = EpochChallenge::new(
            0,
            CurrentNetwork::hash_bhp1024(&[true; 1024])?.into(),
            CurrentNetwork::COINBASE_PUZZLE_DEGREE,
        )?;
        let message = PoolMessageSC::Notify::<CurrentNetwork>(0, 100000, epoch_challenge);
        check_pool_message_sc(message);

        let message = PoolMessageSC::ShutDown;
        check_pool_message_sc(message);

        let message = PoolMessageSC::Pong;
        check_pool_message_sc(message);

        Ok(())
    }

    #[test]
    fn test_pool_message_cs() -> Result<()> {
        let message = PoolMessageCS::Connect::<CurrentNetwork>(
            0,
            1,
            0,
            1,
            0,
            "my_worker_1".to_string(),
            "215587407@qq.com".to_string(),
        );
        check_pool_message_cs(message);

        let rng = &mut thread_rng();
        let address = Address::<CurrentNetwork>::new(Uniform::rand(rng));
        println!("{}", address);
        let partial_solution =
            PartialSolution::new(address, u64::rand(rng), KZGCommitment(rng.gen()));
        let prover_solution = ProverSolution::new(
            partial_solution,
            KZGProof {
                w: rng.gen(),
                random_v: None,
            },
        );
        let message = PoolMessageCS::Submit::<CurrentNetwork>(0, 0, Data::Object(prover_solution));
        check_pool_message_cs(message);

        let message = PoolMessageCS::DisConnect::<CurrentNetwork>(1);
        check_pool_message_cs(message);

        let message = PoolMessageCS::Ping;
        check_pool_message_cs(message);
        Ok(())
    }
}
