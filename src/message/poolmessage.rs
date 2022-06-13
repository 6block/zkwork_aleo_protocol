// Copyright (C) 2019-2022 6block.
// This file is the zk.work pool protocol for Aleo.

// The zkwork_aleo_protol library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.


// You should have received a copy of the GNU General Public License
// along with the zkwork_aleo_protocol library. If not, see <https://www.gnu.org/licenses/>.

use snarkvm::{dpc::posw::PoSWProof, prelude::*};
use snarkos::{network::Data, environment::Environment};

use ::bytes::{Buf, BufMut, BytesMut};
use anyhow::{anyhow, Result};
use std::{io::Write, marker::PhantomData};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Clone, Debug)]
pub enum PoolMessageSC<N: Network, E: Environment> {
    /// ConnectResponse := (is_accecpt, [id])
    ConnectResponse(bool, Option<u32>),
    /// WorkerJob := (share_difficulty, block_template)
    WorkerJob(u64, Data<BlockTemplate<N>>),
    /// ShutDown := ()
    ShutDown,
    /// Unused
    #[allow(unused)]
    Unused(PhantomData<E>),
}

impl<N: Network, E: Environment> PoolMessageSC<N, E> {
    /// Returns the messge name
    #[inline]
    pub fn name(&self) -> &str {
        match self {
            Self::ConnectResponse(..) => "PoolConnectResponse",
            Self::WorkerJob(..) => "PoolWorkerJob",
            Self::ShutDown => "PoolShutDown",
            Self::Unused(..) => "Unused",
        }
    }

    /// Returns the message ID.
    #[inline]
    pub fn id(&self) -> u8 {
        match self {
            Self::ConnectResponse(..) => 0,
            Self::WorkerJob(..) => 1,
            Self::ShutDown => 2,
            Self::Unused(..) => 3,
        }
    }

    /// Returns the message data as bytes.
    #[inline]
    pub fn serialize_data_into<W: Write>(&self, writer: &mut W) -> Result<()> {
        match self {
            Self::ConnectResponse(is_accept, id) => match is_accept {
                true => match id {
                    Some(id) => {
                        writer.write_all(&[1u8])?;
                        writer.write_all(&id.to_le_bytes())?;
                        Ok(())
                    }
                    None => return Err(anyhow!("ConnectResponse: Invalid id")),
                },
                false => Ok(()),
            },
            Self::WorkerJob(share_difficulty, block_template) => {
                bincode::serialize_into(&mut *writer, share_difficulty)?;
                block_template.serialize_blocking_into(writer)
            }
            Self::ShutDown => Ok(()),
            Self::Unused(_) => Ok(()),
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
                    0 => Self::ConnectResponse(false, None),
                    1 => Self::ConnectResponse(true, Some(u32::from_le_bytes([data[1], data[2], data[3], data[4]]))),
                    _ => return Err(anyhow!("Invalid 'ConnectResponse' message: {:?} {:?}", buffer, data)),
                },
            },
            1 => Self::WorkerJob(bincode::deserialize(&data[0..8])?, Data::Buffer(data[8..].to_vec().into())),
            2 => match data.is_empty() {
                true => Self::ShutDown,
                false => return Err(anyhow!("Invalid 'ShutDown' message: {:?} {:?}", buffer, data)),
            },
            _ => return Err(anyhow!("Invalid message ID {}", id)),
        };

        Ok(message)
    }
}

impl<N: Network, E: Environment> Encoder<PoolMessageSC<N, E>> for PoolMessageSC<N, E> {
    type Error = anyhow::Error;

    fn encode(&mut self, message: PoolMessageSC<N, E>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&0u32.to_le_bytes());
        message.serialize_into(&mut dst.writer())?;
        let len_slice = (dst[4..].len() as u32).to_le_bytes();
        dst[..4].copy_from_slice(&len_slice);
        Ok(())
    }
}

impl<N: Network, E: Environment> Decoder for PoolMessageSC<N, E> {
    type Error = std::io::Error;
    type Item = PoolMessageSC<N, E>;

    fn decode(&mut self, source: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if source.len() < 4 {
            return Ok(None);
        }
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&source[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;
        // Check that the length is not too large to avoid a denial of
        // service attack where the node server runs out of memory.
        if length > E::MAXIMUM_MESSAGE_SIZE {
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
pub enum PoolMessageCS<N: Network, E: Environment> {
    /// Connect := (type, version(major, minor, patch), name)
    Connect(u8, u8, u8, u8, String),
    /// ShareBlock := (id, address, nonce, previous_block_hash, proof)
    ShareBlock(u32, Address<N>, N::PoSWNonce, N::BlockHash, Data<PoSWProof<N>>),
    /// DisConnect := (id)
    DisConnect(u32),
    // Unused
    #[allow(unused)]
    Unused(PhantomData<E>),
}

impl<N: Network, E: Environment> PoolMessageCS<N, E> {
    /// Returns the messge name
    #[inline]
    pub fn name(&self) -> &str {
        match self {
            Self::Connect(..) => "PoolConnect",
            Self::ShareBlock(..) => "PoolShareBlock",
            Self::DisConnect(..) => "PoolDisconnect",
            Self::Unused(..) => "Unused",
        }
    }

    /// Returns the message ID.
    pub fn id(&self) -> u8 {
        match self {
            Self::Connect(..) => 128,
            Self::ShareBlock(..) => 129,
            Self::DisConnect(..) => 130,
            Self::Unused(..) => 131,
        }
    }

    /// Returns the message data as bytes.
    #[inline]
    pub fn serialize_data_into<W: Write>(&self, writer: &mut W) -> Result<()> {
        match self {
            Self::Connect(worker_type, v_major, v_minor, v_patch, custom_name) => {
                writer.write_all(&[*worker_type])?;
                writer.write_all(&[*v_major])?;
                writer.write_all(&[*v_minor])?;
                writer.write_all(&[*v_patch])?;
                bincode::serialize_into(&mut *writer, custom_name)?;
                Ok(())
            }
            Self::ShareBlock(id, address, nonce, previous_block_hash, proof) => {
                bincode::serialize_into(&mut *writer, id)?;
                bincode::serialize_into(&mut *writer, address)?;
                bincode::serialize_into(&mut *writer, nonce)?;
                bincode::serialize_into(&mut *writer, previous_block_hash)?;
                proof.serialize_blocking_into(writer)
            }
            Self::DisConnect(id) => {
                bincode::serialize_into(&mut *writer, id)?;
                Ok(())
            }
            Self::Unused(_) => Ok(()),
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
            128 => Self::Connect(data[0], data[1], data[2], data[3], bincode::deserialize(&data[4..])?),
            129 => Self::ShareBlock(
                bincode::deserialize(&data[0..4])?,
                bincode::deserialize(&data[4..36])?,
                bincode::deserialize(&data[36..68])?,
                bincode::deserialize(&data[68..100])?,
                Data::Buffer(data[100..].to_vec().into()),
            ),
            130 => Self::DisConnect(bincode::deserialize(data)?),
            _ => return Err(anyhow!("Invalid message ID {}", id)),
        };

        Ok(message)
    }
}

impl<N: Network, E: Environment> Encoder<PoolMessageCS<N, E>> for PoolMessageCS<N, E> {
    type Error = anyhow::Error;

    fn encode(&mut self, message: PoolMessageCS<N, E>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(&0u32.to_le_bytes());
        message.serialize_into(&mut dst.writer())?;
        let len_slice = (dst[4..].len() as u32).to_le_bytes();
        dst[..4].copy_from_slice(&len_slice);
        Ok(())
    }
}

impl<N: Network, E: Environment> Decoder for PoolMessageCS<N, E> {
    type Error = std::io::Error;
    type Item = PoolMessageCS<N, E>;

    fn decode(&mut self, source: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if source.len() < 4 {
            return Ok(None);
        }
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&source[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;
        // Check that the length is not too large to avoid a denial of
        // service attack where the node server runs out of memory.
        if length > E::MAXIMUM_MESSAGE_SIZE {
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
    use snarkos::Miner;
    use ::rand::thread_rng;
    use snarkvm::dpc::testnet2::Testnet2;
    fn check_pool_message_sc(message: PoolMessageSC<Testnet2, Miner<Testnet2>>) {
        println!("{:?}", message);
        let mut buffer = BytesMut::new();
        let _ = PoolMessageSC::Unused(PhantomData).encode(message, &mut buffer);
        println!("{:?}", buffer);
        let message1 = PoolMessageSC::Unused::<Testnet2, Miner<Testnet2>>(PhantomData)
            .decode(&mut buffer.clone())
            .unwrap()
            .unwrap();
        println!("{:?}", message1);
        let mut buffer_2 = BytesMut::new();
        let _ = PoolMessageSC::Unused(PhantomData).encode(message1.clone(), &mut buffer_2);
        assert_eq!(buffer, buffer_2);
    }

    fn check_pool_message_cs(message: PoolMessageCS<Testnet2, Miner<Testnet2>>) {
        println!("message: {:?}", message);
        let mut buffer = BytesMut::new();
        let _ = PoolMessageCS::Unused(PhantomData).encode(message, &mut buffer);
        println!("buffer: {:?}", buffer);
        let message1 = PoolMessageCS::Unused::<Testnet2, Miner<Testnet2>>(PhantomData)
            .decode(&mut buffer.clone())
            .unwrap()
            .unwrap();
        println!("message: {:?}", message1);
        let mut buffer_2 = BytesMut::new();
        let _ = PoolMessageCS::Unused(PhantomData).encode(message1.clone(), &mut buffer_2);
        println!("buffer: {:?}", buffer_2);
        assert_eq!(buffer, buffer_2);
    }

    #[test]
    fn test_pool_message_sc() {
        let message = PoolMessageSC::ConnectResponse::<Testnet2, Miner<Testnet2>>(true, Some(1));
        check_pool_message_sc(message);

        let block = Testnet2::genesis_block();
        let block_template = BlockTemplate::new(
            block.previous_block_hash(),
            block.height(),
            block.timestamp(),
            block.difficulty_target(),
            block.cumulative_weight(),
            block.previous_ledger_root(),
            block.transactions().clone(),
            block.to_coinbase_transaction().unwrap().to_records().next().unwrap(),
        );
        let message = PoolMessageSC::WorkerJob::<Testnet2, Miner<Testnet2>>(100000, Data::Object(block_template));
        check_pool_message_sc(message);

        let message = PoolMessageSC::ShutDown::<Testnet2, Miner<Testnet2>>;
        check_pool_message_sc(message);
    }

    #[test]
    fn test_pool_message_cs() {
        let message = PoolMessageCS::Connect::<Testnet2, Miner<Testnet2>>(0, 0, 1, 0, "my_worker_1".to_string());
        check_pool_message_cs(message);

        let rng = &mut thread_rng();
        let private_key = PrivateKey::new(rng);
        let address: Address<Testnet2> = private_key.into();
        println!("{}", address);
        let block = Testnet2::genesis_block();
        let nonce = block.nonce();
        let proof = block.header().proof();
        let previous_block_hash = block.previous_block_hash();
        let message =
            PoolMessageCS::ShareBlock::<Testnet2, Miner<Testnet2>>(0, address, nonce, previous_block_hash, Data::Object((*proof).clone()));
        check_pool_message_cs(message);

        let message = PoolMessageCS::DisConnect::<Testnet2, Miner<Testnet2>>(1);
        check_pool_message_cs(message);
    }
}
