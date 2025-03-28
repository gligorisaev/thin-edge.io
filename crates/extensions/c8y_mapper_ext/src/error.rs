use crate::entity_cache::InvalidExternalIdError;
use crate::supported_operations::OperationsError;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_http_proxy::messages::C8YRestError;
use plugin_sm::operation_logs::OperationLogsError;
use std::path::PathBuf;
use tedge_api::measurement::ThinEdgeJsonSerializationError;
use tedge_config::TEdgeConfigError;
use tedge_mqtt_ext::MqttError;
use tedge_utils::file::FileError;
use tedge_utils::size_threshold::SizeThresholdExceededError;

// allowing enum_variant_names due to a False positive where it is
// detected that "all variants have the same prefix: `From`"
#[allow(clippy::enum_variant_names)]
#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error(transparent)]
    FromMqttClient(#[from] MqttError),

    #[error(transparent)]
    FromTEdgeConfig(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromNotifyFs(#[from] tedge_utils::notify::NotifyStreamError),

    #[error(transparent)]
    FromStdIo(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to convert a message on topic '{topic}': {error:#}")]
pub struct MessageConversionError {
    pub error: ConversionError,
    pub topic: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error(transparent)]
    FromMapper(#[from] MapperError),

    #[error(transparent)]
    FromCumulocityJsonError(#[from] crate::json::CumulocityJsonError),

    #[error(transparent)]
    FromCumulocityMapperError(#[from] CumulocityMapperError),

    #[error(transparent)]
    FromCumulocitySmartRestMapperError(#[from] c8y_api::smartrest::error::SMCumulocityMapperError),

    #[error(transparent)]
    FromThinEdgeJsonSerialization(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    FromThinEdgeJsonAlarmDeserialization(#[from] tedge_api::alarm::ThinEdgeAlarmDeserializerError),

    #[error(transparent)]
    FromThinEdgeJsonEventDeserialization(
        #[from] tedge_api::event::error::ThinEdgeJsonDeserializerError,
    ),

    #[error(transparent)]
    FromThinEdgeJsonParser(#[from] tedge_api::measurement::ThinEdgeJsonParserError),

    #[error(transparent)]
    SizeThresholdExceeded(#[from] SizeThresholdExceededError),

    #[error(transparent)]
    FromMqttClient(#[from] MqttError),

    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_api::smartrest::error::SmartRestSerializerError),

    #[error(transparent)]
    FromSmartRestDeserializerError(#[from] c8y_api::smartrest::error::SmartRestDeserializerError),

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromStdIo(#[from] std::io::Error),

    #[error("Error converting json option")]
    FromOptionError,

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    FromTimeFormatError(#[from] time::error::Format),

    #[error("The payload {payload} received on {topic} after translation is {actual_size} greater than the threshold size of {threshold}.")]
    TranslatedSizeExceededThreshold {
        payload: String,
        topic: String,
        actual_size: usize,
        threshold: usize,
    },

    #[error(transparent)]
    FromOperationLogsError(#[from] plugin_sm::operation_logs::OperationLogsError),

    #[error("The given Child ID '{id}' is not registered with Cumulocity. To send the events to the child device, it has to be registered first.")]
    ChildDeviceNotRegistered { id: String },

    #[error("Failed to extract the child device name from file path : {dir}")]
    DirPathComponentError { dir: PathBuf },

    #[error(transparent)]
    FromC8YRestError(#[from] C8YRestError),

    #[error(transparent)]
    FileError(#[from] FileError),

    #[error(transparent)]
    FromC8yAlarmError(#[from] c8y_api::json_c8y::C8yAlarmError),

    #[error(transparent)]
    FromEntityStoreError(#[from] tedge_api::entity_store::Error),

    #[error(transparent)]
    FromEntityCacheError(#[from] crate::entity_cache::Error),

    #[error(transparent)]
    InvalidExternalIdError(#[from] InvalidExternalIdError),

    #[error("Unexpected error: {0:?}")]
    UnexpectedError(#[from] anyhow::Error),

    #[error("The provided entity: {0} was not found and could not be auto-registered either, because it is disabled")]
    AutoRegistrationDisabled(String),

    #[error(transparent)]
    ChannelError(#[from] tedge_actors::ChannelError),
}

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum CumulocityMapperError {
    #[error(transparent)]
    FromEntityStore(#[from] tedge_api::entity_store::Error),

    #[error(transparent)]
    FromEntityCacheError(#[from] crate::entity_cache::Error),

    #[error(transparent)]
    InvalidTopicError(#[from] tedge_api::TopicError),

    #[error(transparent)]
    InvalidThinEdgeJson(#[from] tedge_api::SoftwareError),

    #[error(transparent)]
    FromElapsed(#[from] tokio::time::error::Elapsed),

    #[error(transparent)]
    FromMqttClient(#[from] tedge_mqtt_ext::MqttError),

    #[error(transparent)]
    FromSmartRestSerializer(#[from] SmartRestSerializerError),

    #[error(transparent)]
    FromSmartRestDeserializer(#[from] SmartRestDeserializerError),

    #[error(transparent)]
    FromC8yJsonOverMqttDeserializerError(
        #[from] c8y_api::json_c8y_deserializer::C8yJsonOverMqttDeserializerError,
    ),

    #[error(transparent)]
    FromSmCumulocityMapperError(#[from] SMCumulocityMapperError),

    #[error(transparent)]
    FromTedgeConfig(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromTimeFormat(#[from] time::error::Format),

    #[error(transparent)]
    FromTimeParse(#[from] time::error::Parse),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromSerde(#[from] serde_json::Error),

    #[error("Operation execution failed: {error_message}. Command: {command}. Operation name: {operation_name}")]
    ExecuteFailed {
        error_message: String,
        command: String,
        operation_name: String,
    },

    #[error("Failed to read the child device operations in directory: {dir}")]
    ReadDirError { dir: std::path::PathBuf },

    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromOperationLogs(#[from] OperationLogsError),

    #[error(transparent)]
    TedgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromC8YRestError(#[from] C8YRestError),

    #[error(transparent)]
    ChannelError(#[from] tedge_actors::ChannelError),

    #[error("Error occurred while preprocessing custom operation handler {operation}. Reason: {err_msg}")]
    JsonCustomOperationHandlerError { operation: String, err_msg: String },
}
