/* udf.rs
   File generated by the "User" based on vision, with limited knowledge about the other parts of the framework

        Knowledge needed:
        1) UserDefinedFunction trait and how to implement it
        2) vertex's apply_function() function and their invoking protocols

   Author: Binghong(Leo) Li
   Creation Date: 1/14/2023
*/

use async_trait::async_trait;
use std::ops::AddAssign;

use crate::graph::*;
use crate::vertex::*;
use crate::UserDefinedFunction;

/* *********** Starting of User's Playground *********** */

// Summing the entire graph

/*
   Data<isize> operations
*/
impl AddAssign<isize> for Data<isize> {
    fn add_assign(&mut self, other: isize) {
        self.0 += other;
    }
}

// UDF Struct
pub struct GraphSum;
#[async_trait]
impl UserDefinedFunction<isize, Option<u64>> for GraphSum {
    async fn execute(
        &self,
        vertex: &Vertex<isize>,
        graph: &Graph<isize>,
        aux_info: Option<u64>,
    ) -> isize {
        let mut count = Data(0);
        count += vertex.get_val().as_ref().unwrap().0;

        for sub_graph_root_id in vertex.children().iter() {
            count += graph
                .get(sub_graph_root_id)
                .expect("node not found")
                .apply_function(self, graph, aux_info)
                .await;
        }
        count.0
    }
}

// summing adjacent nodes