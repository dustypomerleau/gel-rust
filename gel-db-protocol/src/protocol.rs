use gel_protogen::prelude::*;

pub use gel_protogen::prelude;

message_group!(
    EdgeDBBackend: Message = [
        AuthenticationOk,
        AuthenticationRequiredSASLMessage,
        AuthenticationSASLContinue,
        AuthenticationSASLFinal,
        ServerKeyData,
        ParameterStatus,
        ServerHandshake,
        ReadyForCommand,
        RestoreReady,
        CommandComplete,
        CommandDataDescription,
        StateDataDescription,
        Data,
        DumpHeader,
        DumpBlock,
        ErrorResponse,
        LogMessage
    ]
);

message_group!(
    EdgeDBFrontend2: Message = [
        ClientHandshake,
        AuthenticationSASLInitialResponse,
        AuthenticationSASLResponse,
        Parse2,
        Execute2,
        Sync,
        Terminate,
        Dump2,
        Restore,
        RestoreBlock,
        RestoreEof
    ]
);

message_group!(
    EdgeDBFrontend: Message = [
        ClientHandshake,
        AuthenticationSASLInitialResponse,
        AuthenticationSASLResponse,
        Parse,
        Execute,
        Sync,
        Terminate,
        Dump3,
        Restore,
        RestoreBlock,
        RestoreEof
    ]
);

protocol!(

/// A generic base for all EdgeDB mtype/mlen-style messages.
struct Message<'a> {
    /// Identifies the message.
    mtype: u8,
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message contents.
    data: Rest<'a>,
}

/// The `ErrorResponse` struct represents an error message sent from the server.
struct ErrorResponse<'a>: Message {
    /// Identifies the message as an error response.
    mtype: u8 = 'E',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message severity.
    severity: u8,
    /// Message code.
    error_code: u32,
    /// Error message.
    message: LString<'a>,
    /// Error attributes.
    attributes: Array<'a, i16, KeyValue<'a>>,
}

/// The `LogMessage` struct represents a log message sent from the server.
struct LogMessage<'a>: Message {
    /// Identifies the message as a log message.
    mtype: u8 = 'L',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message severity.
    severity: u8,
    /// Message code.
    code: i32,
    /// Message text.
    text: LString<'a>,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
}

/// The `ReadyForCommand` struct represents a message indicating the server is ready for a new command.
struct ReadyForCommand<'a>: Message {
    /// Identifies the message as ready for command.
    mtype: u8 = 'Z',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// Transaction state.
    transaction_state: TransactionState,
}

/// The `RestoreReady` struct represents a message indicating the server is ready for restore.
struct RestoreReady<'a>: Message {
    /// Identifies the message as restore ready.
    mtype: u8 = '+',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message headers.
    headers: Array<'a, i16, KeyValue<'a>>,
    /// Number of parallel jobs for restore.
    jobs: u16,
}

/// The `CommandComplete` struct represents a message indicating a command has completed.
struct CommandComplete<'a>: Message {
    /// Identifies the message as command complete.
    mtype: u8 = 'C',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// A bit mask of allowed capabilities.
    capabilities: u64,
    /// Command status.
    status: LString<'a>,
    /// State data descriptor ID.
    state_typedesc_id: Uuid,
    /// Encoded state data.
    state_data: Array<'a, u32, u8>,
}

/// The `CommandDataDescription` struct represents a description of command data.
struct CommandDataDescription<'a>: Message {
    /// Identifies the message as command data description.
    mtype: u8 = 'T',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// A bit mask of allowed capabilities.
    capabilities: u64,
    /// Actual result cardinality.
    result_cardinality: u8,
    /// Argument data descriptor ID.
    input_typedesc_id: Uuid,
    /// Argument data descriptor.
    input_typedesc: Array<'a, u32, u8>,
    /// Output data descriptor ID.
    output_typedesc_id: Uuid,
    /// Output data descriptor.
    output_typedesc: Array<'a, u32, u8>,
}

/// The `StateDataDescription` struct represents a description of state data.
struct StateDataDescription<'a>: Message {
    /// Identifies the message as state data description.
    mtype: u8 = 's',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Updated state data descriptor ID.
    typedesc_id: Uuid,
    /// State data descriptor.
    typedesc: Array<'a, u32, u8>,
}

/// The `Data` struct represents a data message.
struct Data<'a>: Message {
    /// Identifies the message as data.
    mtype: u8 = 'D',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Encoded output data array.
    data: Array<'a, i16, DataElement<'a>>,
}

/// The `DumpHeader` struct represents a dump header message.
struct DumpHeader<'a>: Message {
    /// Identifies the message as dump header.
    mtype: u8 = '@',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Dump attributes.
    attributes: Array<'a, i16, KeyValue<'a>>,
    /// Major version of EdgeDB.
    major_ver: i16,
    /// Minor version of EdgeDB.
    minor_ver: i16,
    /// Schema.
    schema_ddl: LString<'a>,
    /// Type identifiers.
    types: Array<'a, i32, DumpTypeInfo<'a>>,
    /// Object descriptors.
    descriptors: Array<'a, i32, DumpObjectDesc<'a>>,
}

/// The `DumpBlock` struct represents a dump block message.
struct DumpBlock<'a>: Message {
    /// Identifies the message as dump block.
    mtype: u8 = '=',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Dump attributes.
    attributes: Array<'a, i16, KeyValue<'a>>,
}

/// The `ServerKeyData` struct represents server key data.
struct ServerKeyData<'a>: Message {
    /// Identifies the message as server key data.
    mtype: u8 = 'K',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Key data.
    data: [u8; 32],
}

/// The `ParameterStatus` struct represents a parameter status message.
struct ParameterStatus<'a>: Message {
    /// Identifies the message as parameter status.
    mtype: u8 = 'S',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Parameter name.
    name: Array<'a, u32, u8>,
    /// Parameter value.
    value: Array<'a, u32, u8>,
}

/// The `ServerHandshake` struct represents a server handshake message.
struct ServerHandshake<'a>: Message {
    /// Identifies the message as server handshake.
    mtype: u8 = 'v',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Maximum supported or client-requested protocol major version.
    major_ver: u16,
    /// Maximum supported or client-requested protocol minor version.
    minor_ver: u16,
    /// Supported protocol extensions.
    extensions: Array<'a, i16, ProtocolExtension<'a>>,
}

/// The `AuthenticationRequired` struct represents an authentication message.
struct Authentication<'a>: Message {
    /// Identifies the message as authentication OK.
    mtype: u8 = 'R',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// The type of authentication message.
    auth_status: i32,
    /// The authentication data.
    data: Rest<'a>,
}

/// The `AuthenticationOk` struct represents a successful authentication message.
struct AuthenticationOk<'a>: Message {
    /// Identifies the message as authentication OK.
    mtype: u8 = 'R',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Specifies that this message contains a successful authentication indicator.
    auth_status: i32 = 0x0,
}

/// The `AuthenticationRequiredSASLMessage` struct represents a SASL authentication request.
struct AuthenticationRequiredSASLMessage<'a>: Message {
    /// Identifies the message as authentication required SASL.
    mtype: u8 = 'R',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Specifies that this message contains a SASL authentication request.
    auth_status: i32 = 0x0A,
    /// A list of supported SASL authentication methods.
    methods: Array<'a, i32, LString<'a>>,
}

/// The `AuthenticationSASLContinue` struct represents a SASL challenge.
struct AuthenticationSASLContinue<'a>: Message {
    /// Identifies the message as authentication SASL continue.
    mtype: u8 = 'R',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Specifies that this message contains a SASL challenge.
    auth_status: i32 = 0x0B,
    /// Mechanism-specific SASL data.
    sasl_data: Array<'a, u32, u8>,
}

/// The `AuthenticationSASLFinal` struct represents the completion of SASL authentication.
struct AuthenticationSASLFinal<'a>: Message {
    /// Identifies the message as authentication SASL final.
    mtype: u8 = 'R',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Specifies that SASL authentication has completed.
    auth_status: i32 = 0x0C,
    /// SASL data.
    sasl_data: Array<'a, u32, u8>,
}

/// The `Dump` struct represents a dump message from the client.
struct Dump<'a>: Message {
    /// Identifies the message as dump.
    mtype: u8 = '>',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
}

/// The `Dump2` struct represents a dump message from the client.
struct Dump2<'a>: Message {
    /// Identifies the message as dump.
    mtype: u8 = '>',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message headers.
    headers: Array<'a, i16, KeyValue<'a>>,
}

/// The `Dump3` struct represents a dump message from the client.
struct Dump3<'a>: Message {
    /// Identifies the message as dump.
    mtype: u8 = '>',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// A bit mask of dump options.
    flags: u64,
}

/// The `Sync` struct represents a synchronization message from the client.
struct Sync<'a>: Message {
    /// Identifies the message as sync.
    mtype: u8 = 'S',
    /// Length of message contents in bytes, including self.
    mlen: len,
}

/// The `Restore` struct represents a restore message from the client.
struct Restore<'a>: Message {
    /// Identifies the message as restore.
    mtype: u8 = '<',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Restore headers.
    headers: Array<'a, i16, KeyValue<'a>>,
    /// Number of parallel jobs for restore.
    jobs: u16,
    /// Original DumpHeader packet data excluding mtype and message_length.
    data: Rest<'a>,
}

/// The `RestoreBlock` struct represents a restore block message from the client.
struct RestoreBlock<'a>: Message {
    /// Identifies the message as restore block.
    mtype: u8 = '=',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Original DumpBlock packet data excluding mtype and message_length.
    block_data: Array<'a, u32, u8>,
}

/// The `RestoreEof` struct represents the end of restore message from the client.
struct RestoreEof<'a>: Message {
    /// Identifies the message as restore EOF.
    mtype: u8 = '.',
    /// Length of message contents in bytes, including self.
    mlen: len,
}

/// The `Parse` struct represents a parse message from the client.
struct Parse2<'a>: Message {
    /// Identifies the message as parse.
    mtype: u8 = 'P',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// A bit mask of allowed capabilities.
    allowed_capabilities: u64,
    /// A bit mask of query options.
    compilation_flags: u64,
    /// Implicit LIMIT clause on returned sets.
    implicit_limit: u64,
    /// Data output format.
    output_format: IoFormat,
    /// Expected result cardinality.
    expected_cardinality: u8,
    /// Command text.
    command_text: LString<'a>,
    /// State data descriptor ID.
    state_typedesc_id: Uuid,
    /// Encoded state data.
    state_data: Array<'a, u32, u8>,
}

/// The `Parse` struct represents a parse message from the client.
struct Parse<'a>: Message {
    /// Identifies the message as parse.
    mtype: u8 = 'P',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// A bit mask of allowed capabilities.
    allowed_capabilities: u64,
    /// A bit mask of query options.
    compilation_flags: u64,
    /// Implicit LIMIT clause on returned sets.
    implicit_limit: u64,
    /// Input language.
    input_language: InputLanguage,
    /// Data output format.
    output_format: IoFormat,
    /// Expected result cardinality.
    expected_cardinality: u8,
    /// Command text.
    command_text: LString<'a>,
    /// State data descriptor ID.
    state_typedesc_id: Uuid,
    /// Encoded state data.
    state_data: Array<'a, u32, u8>,
}

/// The `Execute` struct represents an execute message from the client.
struct Execute<'a>: Message {
    /// Identifies the message as execute.
    mtype: u8 = 'O',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// A bit mask of allowed capabilities.
    allowed_capabilities: u64,
    /// A bit mask of query options.
    compilation_flags: u64,
    /// Implicit LIMIT clause on returned sets.
    implicit_limit: u64,
    /// Input language.
    input_language: InputLanguage,
    /// Data output format.
    output_format: IoFormat,
    /// Expected result cardinality.
    expected_cardinality: u8,
    /// Command text.
    command_text: LString<'a>,
    /// State data descriptor ID.
    state_typedesc_id: Uuid,
    /// Encoded state data.
    state_data: Array<'a, u32, u8>,
    /// Argument data descriptor ID.
    input_typedesc_id: Uuid,
    /// Output data descriptor ID.
    output_typedesc_id: Uuid,
    /// Encoded argument data.
    arguments: Array<'a, u32, u8>,
}

/// The `ClientHandshake` struct represents a client handshake message.
struct ClientHandshake<'a>: Message {
    /// Identifies the message as client handshake.
    mtype: u8 = 'V',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Requested protocol major version.
    major_ver: u16,
    /// Requested protocol minor version.
    minor_ver: u16,
    /// Connection parameters.
    params: Array<'a, i16, ConnectionParam<'a>>,
    /// Requested protocol extensions.
    extensions: Array<'a, i16, ProtocolExtension<'a>>,
}

/// The `Terminate` struct represents a termination message from the client.
struct Terminate<'a>: Message {
    /// Identifies the message as terminate.
    mtype: u8 = 'X',
    /// Length of message contents in bytes, including self.
    mlen: len,
}

/// The `AuthenticationSASLInitialResponse` struct represents the initial SASL response from the client.
struct AuthenticationSASLInitialResponse<'a>: Message {
    /// Identifies the message as authentication SASL initial response.
    mtype: u8 = 'p',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Name of the SASL authentication mechanism that the client selected.
    method: LString<'a>,
    /// Mechanism-specific "Initial Response" data.
    sasl_data: Array<'a, u32, u8>,
}

/// The `AuthenticationSASLResponse` struct represents a SASL response from the client.
struct AuthenticationSASLResponse<'a>: Message {
    /// Identifies the message as authentication SASL response.
    mtype: u8 = 'r',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Mechanism-specific response data.
    sasl_data: Array<'a, u32, u8>,
}

/// The `KeyValue` struct represents a key-value pair.
struct KeyValue<'a> {
    /// Key code (specific to the type of the Message).
    code: u16,
    /// Value data.
    value: Array<'a, u32, u8>,
}

/// The `Annotation` struct represents an annotation.
struct Annotation<'a> {
    /// Name of the annotation.
    name: LString<'a>,
    /// Value of the annotation (in JSON format).
    value: LString<'a>,
}

/// The `DataElement` struct represents a data element.
struct DataElement<'a> {
    /// Encoded output data.
    data: Array<'a, i32, u8>,
}

/// The `DumpTypeInfo` struct represents type information in a dump.
struct DumpTypeInfo<'a> {
    /// Type name.
    type_name: LString<'a>,
    /// Type class.
    type_class: LString<'a>,
    /// Type ID.
    type_id: Uuid,
}

/// The `DumpObjectDesc` struct represents an object descriptor in a dump.
struct DumpObjectDesc<'a> {
    /// Object ID.
    object_id: Uuid,
    /// Description.
    description: Array<'a, u32, u8>,
    /// Dependencies.
    dependencies: Array<'a, i16, Uuid>,
}

/// The `ProtocolExtension` struct represents a protocol extension.
struct ProtocolExtension<'a> {
    /// Extension name.
    name: LString<'a>,
    /// A set of extension annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
}

/// The `ConnectionParam` struct represents a connection parameter.
struct ConnectionParam<'a> {
    /// Parameter name.
    name: LString<'a>,
    /// Parameter value.
    value: LString<'a>,
}

/// Legacy version of [`Execute`] without `input_language`.
struct Execute2<'a>: Message {
    /// Identifies the message as execute.
    mtype: u8 = 'O',
    /// Length of message contents in bytes, including self.
    mlen: len,
    /// Message annotations.
    annotations: Array<'a, i16, Annotation<'a>>,
    /// A bit mask of allowed capabilities.
    allowed_capabilities: u64,
    /// A bit mask of query options.
    compilation_flags: u64,
    /// Implicit LIMIT clause on returned sets.
    implicit_limit: u64,
    /// Data output format.
    output_format: IoFormat,
    /// Expected result cardinality.
    expected_cardinality: u8,
    /// Command text.
    command_text: LString<'a>,
    /// State data descriptor ID.
    state_typedesc_id: Uuid,
    /// Encoded state data.
    state_data: Array<'a, u32, u8>,
    /// Argument data descriptor ID.
    input_typedesc_id: Uuid,
    /// Output data descriptor ID.
    output_typedesc_id: Uuid,
    /// Encoded argument data.
    arguments: Array<'a, u32, u8>,
}
);

/// Data format.
#[derive(Copy, Clone, Protocol, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum IoFormat {
    Binary = 0x62,
    Json = 0x6a,
    JsonElements = 0x4a,
    #[default]
    None = 0x6e,
}

/// Input language.
#[derive(Copy, Clone, Protocol, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum InputLanguage {
    #[default]
    None = 0,
    EdgeQL = 0x45,
    SQL = 0x53,
}

/// The state of the current transaction.
#[derive(Copy, Clone, Protocol, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum TransactionState {
    #[default]
    NotInTransaction = 0x49,
    InTransaction = 0x54,
    InFailedTransaction = 0x45,
}

/// The cardinality of the result set.
#[derive(Copy, Clone, Protocol, Default, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Cardinality {
    #[default]
    NoResult = 0x6e,
    AtMostOne = 0x6f,
    One = 0x41,
    Many = 0x6d,
    AtLeastOne = 0x4d,
}

impl Cardinality {
    pub fn is_optional(&self) -> bool {
        use Cardinality::*;
        match self {
            NoResult => true,
            AtMostOne => true,
            One => false,
            Many => true,
            AtLeastOne => false,
        }
    }
}
