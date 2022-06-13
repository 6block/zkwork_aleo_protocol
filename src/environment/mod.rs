// Copyright (C) 2019-2022 6block.
// This file is the zk.work pool protocol for Aleo.

// The zkwork_aleo_protol library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.


// You should have received a copy of the GNU General Public License
// along with the zkwork_aleo_protocol library. If not, see <https://www.gnu.org/licenses/>.

use snarkvm::dpc::Network;
use snarkos::{
    environment::Environment,
    helpers::NodeType,
};
use std::{
    fmt::Debug,
    marker::PhantomData,
};


#[derive(Clone, Debug, Default)]
pub struct SixPoolWorker<N: Network>(PhantomData<N>);

#[rustfmt::skip]
impl<N: Network> Environment for SixPoolWorker<N> {
    type Network = N;
    const NODE_TYPE: NodeType = NodeType::Miner;
    const MINIMUM_NUMBER_OF_PEERS: usize = 0;
    const MAXIMUM_NUMBER_OF_PEERS: usize = 0;
}

#[derive(Clone, Debug, Default)]
pub struct SixPoolAgent<N: Network>(PhantomData<N>);

#[rustfmt::skip]
impl<N: Network> Environment for SixPoolAgent<N> {
    type Network = N;
    const NODE_TYPE: NodeType = NodeType::Miner;
    const MINIMUM_NUMBER_OF_PEERS: usize = 0;
    const MAXIMUM_NUMBER_OF_PEERS: usize = 0;
}

#[derive(Clone, Debug, Default)]
pub struct SixPoolWorkerTrial<N: Network>(PhantomData<N>);

#[rustfmt::skip]
impl<N: Network> Environment for SixPoolWorkerTrial<N> {
    type Network = N;
    const NODE_TYPE: NodeType = NodeType::Miner;
    const MINIMUM_NUMBER_OF_PEERS: usize = 0;
    const MAXIMUM_NUMBER_OF_PEERS: usize = 0;
}

#[derive(Clone, Debug, Default)]
pub struct SixPoolAgentTrial<N: Network>(PhantomData<N>);

#[rustfmt::skip]
impl<N: Network> Environment for SixPoolAgentTrial<N> {
    type Network = N;
    const NODE_TYPE: NodeType = NodeType::Miner;
    const MINIMUM_NUMBER_OF_PEERS: usize = 0;
    const MAXIMUM_NUMBER_OF_PEERS: usize = 0;
}
