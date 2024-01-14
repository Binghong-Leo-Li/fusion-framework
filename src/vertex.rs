/* vertex.rs
   Contains all the vertex related structs and functions, an layer on top of Vanilla Data

   Author: Binghong(Leo) Li
   Creation Date: 1/14/2023
*/

use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashSet;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{graph::Graph, rpc, UserDefinedFunction};

/* *********** Type Aliases *********** */
pub type VertexID = u32;
pub type MachineID = u32;

/* *********** struct definitions *********** */

/*
   Data Wrapper
*/
#[derive(Serialize)]
pub struct Data<T: DeserializeOwned>(pub T);

/* VertexType
   A vertex is either
        1)  local:      local data
        2)  remote:     remote reference of vertex that lives on another machine/core/node
        3)  borrowed:   brought to local, original copy resides in remote (protected when leased?)
*/
#[derive(Serialize)]
pub enum VertexType<T: DeserializeOwned + Serialize> {
    Local(LocalVertex<T>),
    Remote(RemoteVertex),
    Borrowed(LocalVertex<T>),
    // Note: maybe a (Leased) variant for the future?
}

/*
   Vertex
*/
#[derive(Serialize)]
pub struct Vertex<T: DeserializeOwned + Serialize> {
    pub id: VertexID,
    pub v_type: VertexType<T>,
}
impl<T: DeserializeOwned + Serialize> Vertex<T> {
    /*
        User-Defined_Function Invoker

            T: the output of the UDF, needs to be deserializable for rpc
            F: UDF that defines the execute function
    */
    pub async fn apply_function<F: UserDefinedFunction<T, U>, U>(
        &self,
        udf: &F,
        graph: &Graph<T>,
        auxiliary_information: U,
    ) -> T {
        match &self.v_type {
            VertexType::Local(_) | VertexType::Borrowed(_) => {
                udf.execute(&self, graph, auxiliary_information).await
            }
            VertexType::Remote(remote_vertex) => {
                // Delegate to the remote machine: rpc here
                remote_vertex.remote_execute(self.id, graph).await
            }
        }
    }

    /* Vertex Interfaces
       To allow local_vertex type functions to be called by the outer vertex struct
       Note: this is doable because the functions should never be invoked by a remote_vertex, or there are bugs
    */
    pub fn children(&self) -> &HashSet<VertexID> {
        match &self.v_type {
            VertexType::Local(local_v) | VertexType::Borrowed(local_v) => local_v.children(),
            VertexType::Remote(_) => {
                // this should never be reached
                panic!("Remote Node should not invoke children() function")
            }
        }
    }
    pub fn get_val(&self) -> &Option<Data<T>> {
        match &self.v_type {
            VertexType::Local(local_v) | VertexType::Borrowed(local_v) => local_v.get_data(),
            VertexType::Remote(_) => {
                // this should never be reached
                panic!("Remote Node should not invoke get_val() function")
            }
        }
    }
}

/*
   Vertex that resides locally, or borrowed to be temporarily locally
*/
#[derive(Serialize)]
pub struct LocalVertex<T: DeserializeOwned> {
    incoming_edges: HashSet<VertexID>, // for simulating trees, or DAGs
    outgoing_edges: HashSet<VertexID>, // for simulating trees, or DAGs
    edges: HashSet<VertexID>,          // for simulating general graphs
    data: Option<Data<T>>, // Using option to return the previous value (for error checking, etc.)
    borrowed_in: bool,     // When a node is a borrowed node
    leased_out: bool,      // When the current node is lent out
}
impl<T: DeserializeOwned> LocalVertex<T> {
    /*
       Constructor
    */
    pub fn new(
        incoming: HashSet<VertexID>,
        outgoing: HashSet<VertexID>,
        edges: HashSet<VertexID>,
        data: Option<Data<T>>,
    ) -> Self {
        LocalVertex {
            incoming_edges: incoming,
            outgoing_edges: outgoing,
            edges,
            data,
            borrowed_in: false,
            leased_out: false,
        }
    }

    /*
       Builder/Creator method for easier construction in graph constructors
    */
    pub fn create_vertex(incoming: &[VertexID], outgoing: &[VertexID], data: Data<T>) -> Self {
        LocalVertex::new(
            incoming.iter().cloned().collect(),
            outgoing.iter().cloned().collect(),
            [incoming.to_vec(), outgoing.to_vec()]
                .concat()
                .iter()
                .cloned()
                .collect(),
            Some(data),
        )
    }

    // getters and setters
    pub fn children(&self) -> &HashSet<VertexID> {
        &self.outgoing_edges
    }
    pub fn parents(&self) -> &HashSet<VertexID> {
        &self.incoming_edges
    }
    pub fn edges(&self) -> &HashSet<VertexID> {
        &self.edges
    }
    pub fn get_data(&self) -> &Option<Data<T>> {
        &self.data
    }
    pub fn get_data_mut(&mut self) -> &mut Option<Data<T>> {
        &mut self.data
    }
    pub fn set_data(&mut self, data: Data<T>) -> Option<Data<T>> {
        if self.leased_out {
            None
        } else {
            self.data.replace(data)
        }
    }
}

/*
   Remote References to other vertices
*/
#[derive(Serialize)]
pub struct RemoteVertex {
    location: MachineID,
}
impl RemoteVertex {
    /*
       Constructor
    */
    pub fn new(location: MachineID) -> Self {
        Self { location }
    }

    /*
       RPC for execute
    */
    async fn remote_execute<T>(&self, vertex_id: VertexID, graph: &Graph<T>) -> T
    where
        T: DeserializeOwned + Serialize,
    {
        // The remote machine executes the function and returns the result.
        let rpc_sending_streams = graph.rpc_sending_streams.read().await;
        let mut rpc_sending_stream = rpc_sending_streams
            .get(&self.location)
            .unwrap()
            .lock()
            .await;

        let command = bincode::serialize(&rpc::RPC::Execute(vertex_id)).unwrap();

        rpc_sending_stream.write_all(&command).await.unwrap();

        drop(rpc_sending_stream);
        drop(rpc_sending_streams);

        let mut result_bytes = vec![0u8; std::mem::size_of::<T>()];

        let receiving_streams = graph.receiving_streams.read().await;
        let mut receiving_stream = receiving_streams.get(&self.location).unwrap().lock().await;

        receiving_stream
            .read_exact(&mut result_bytes)
            .await
            .unwrap();

        bincode::deserialize(&result_bytes).unwrap()
    }
}

/*
   Enum to distinguish between different vertex kinds, for graph construction
*/
pub enum VertexKind {
    Local,
    Remote,
    Borrowed,
}