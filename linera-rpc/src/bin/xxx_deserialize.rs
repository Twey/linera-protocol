use linera_base::{
    data_types::{OracleResponse, Round},
    identifiers::{BlobType, ChainDescription, Destination, GenericApplicationId},
    ownership::ChainOwnership,
};
use linera_chain::{
    data_types::{CertificateValue, HashedCertificateValue, Medium, MessageAction},
    manager::ChainManagerInfo,
};
use linera_core::{data_types::CrossChainRequest, node::NodeError};
use linera_execution::{
    system::{AdminOperation, Recipient, SystemChannel, SystemMessage, SystemOperation},
    Message, MessageKind, Operation,
};
use linera_rpc::RpcMessage;
use serde_reflection::{Registry, Result, Samples, Tracer, TracerConfig};


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tracer = Tracer::new(
        TracerConfig::default()
            .record_samples_for_newtype_structs(true)
            .record_samples_for_tuple_structs(true),
    );
    let samples = Samples::new();
    // 1. Record samples for types with custom deserializers.
    // 2. Trace the main entry point(s) + every enum separately.
    tracer.trace_type::<Round>(&samples)?;
    tracer.trace_type::<OracleResponse>(&samples)?;
    tracer.trace_type::<Recipient>(&samples)?;
    tracer.trace_type::<SystemChannel>(&samples)?;
    tracer.trace_type::<SystemOperation>(&samples)?;
    tracer.trace_type::<AdminOperation>(&samples)?;
    tracer.trace_type::<SystemMessage>(&samples)?;
    tracer.trace_type::<Operation>(&samples)?;
    tracer.trace_type::<Message>(&samples)?;
    tracer.trace_type::<MessageAction>(&samples)?;
    tracer.trace_type::<MessageKind>(&samples)?;
    tracer.trace_type::<HashedCertificateValue>(&samples)?;
    tracer.trace_type::<CertificateValue>(&samples)?;
    tracer.trace_type::<Medium>(&samples)?;
    tracer.trace_type::<Destination>(&samples)?;
    tracer.trace_type::<ChainDescription>(&samples)?;
    tracer.trace_type::<ChainOwnership>(&samples)?;
    tracer.trace_type::<GenericApplicationId>(&samples)?;
    tracer.trace_type::<ChainManagerInfo>(&samples)?;
    tracer.trace_type::<CrossChainRequest>(&samples)?;
    tracer.trace_type::<NodeError>(&samples)?;
    tracer.trace_type::<RpcMessage>(&samples)?;
    tracer.trace_type::<BlobType>(&samples)?;

    // tracer.trace_simple_type::<CertificateValue>();
    println!("{}", serde_json::to_string(&tracer.registry().unwrap()).unwrap());
    // let val: CertificateValue = bincode::deserialize(&[1, 0, 0, 0, 228, 118, 24, 127, 109, 223, 235, 157, 88, 140, 123, 69, 211, 223, 51, 77, 85, 1, 214, 73, 155, 63, 154, 213, 89, 92, 174, 134, 204, 225, 106, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 0, 0, 0, 148, 148, 201, 106, 169, 217, 208, 104, 128, 17, 209, 200, 46, 132, 251, 236, 217, 133, 24, 109, 33, 155, 134, 241, 228, 114, 235, 101, 25, 214, 157, 193, 152, 35, 194, 157, 219, 222, 180, 216, 236, 83, 65, 240, 156, 174, 53, 129, 198, 200, 9, 172, 46, 138, 240, 188, 34, 68, 135, 234, 122, 3, 223, 94, 4, 0, 0, 0, 0, 0, 0, 0, 110, 117, 108, 108, 2, 0, 0, 0, 0, 0, 0, 0, 52, 50, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 90, 145, 20, 83, 3, 34, 6, 0, 1, 228, 132, 93, 83, 80, 110, 98, 72, 72, 121, 118, 92, 244, 85, 169, 62, 151, 242, 62, 109, 57, 207, 125, 54, 96, 221, 2, 27, 70, 92, 108, 24, 1, 9, 172, 170, 137, 190, 198, 77, 196, 21, 232, 98, 230, 102, 202, 120, 242, 24, 160, 116, 14, 212, 247, 189, 26, 241, 46, 173, 40, 231, 81, 246, 14, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 118, 24, 127, 109, 223, 235, 157, 88, 140, 123, 69, 211, 223, 51, 77, 85, 1, 214, 73, 155, 63, 154, 213, 89, 92, 174, 134, 204, 225, 106, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 23, 9, 27, 119, 23, 127, 92, 109, 113, 97, 168, 151, 201, 225, 3, 117, 239, 156, 99, 154, 214, 135, 167, 100, 219, 97, 89, 146, 155, 4, 187, 212, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 148, 148, 201, 106, 169, 217, 208, 104, 128, 17, 209, 200, 46, 132, 251, 236, 217, 133, 24, 109, 33, 155, 134, 241, 228, 114, 235, 101, 25, 214, 157, 193, 1, 0, 0, 0, 2, 0, 0, 0, 152, 35, 194, 157, 219, 222, 180, 216, 236, 83, 65, 240, 156, 174, 53, 129, 198, 200, 9, 172, 46, 138, 240, 188, 34, 68, 135, 234, 122, 3, 223, 94, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    // println!("{:#?}", val);

    Ok(())
}
