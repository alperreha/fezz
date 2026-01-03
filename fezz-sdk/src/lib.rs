use serde::{Deserialize, Serialize};
pub use serde_bytes::ByteBuf;

#[repr(C)]
pub struct FezzSlice {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
pub struct FezzOwned {
    pub ptr: *mut u8,
    pub len: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FezzWireHeader {
    pub name: ByteBuf,
    pub value: ByteBuf,
}

impl FezzWireHeader {
    pub fn new<N: AsRef<[u8]>, V: AsRef<[u8]>>(name: N, value: V) -> Self {
        Self {
            name: ByteBuf::from(name.as_ref().to_vec()),
            value: ByteBuf::from(value.as_ref().to_vec()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FezzWireMeta {
    pub trace_id: Option<String>,
    pub deadline_ms: Option<u64>,
    pub client_ip: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FezzWireRequest {
    pub method: String,
    pub scheme: Option<String>,
    pub authority: Option<String>,
    pub path_and_query: String,
    pub headers: Vec<FezzWireHeader>,
    pub body: ByteBuf,
    pub meta: Option<FezzWireMeta>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FezzWireResponse {
    pub status: u16,
    pub headers: Vec<FezzWireHeader>,
    pub body: ByteBuf,
}

impl FezzWireResponse {
    pub fn new<B: Into<Vec<u8>>>(status: u16, headers: Vec<FezzWireHeader>, body: B) -> Self {
        Self {
            status,
            headers,
            body: ByteBuf::from(body.into()),
        }
    }
}

pub fn encode_request(req: &FezzWireRequest) -> Result<Vec<u8>, serde_cbor::Error> {
    serde_cbor::to_vec(req)
}

pub fn decode_request(bytes: &[u8]) -> Result<FezzWireRequest, serde_cbor::Error> {
    serde_cbor::from_slice(bytes)
}

pub fn encode_response(resp: &FezzWireResponse) -> Result<Vec<u8>, serde_cbor::Error> {
    serde_cbor::to_vec(resp)
}

pub fn decode_response(bytes: &[u8]) -> Result<FezzWireResponse, serde_cbor::Error> {
    serde_cbor::from_slice(bytes)
}
