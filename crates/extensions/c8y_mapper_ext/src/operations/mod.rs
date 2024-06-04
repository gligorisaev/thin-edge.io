//! Utilities for executing Cumulocity operations.
//!
//! C8y operations need some special handling by the C8y mapper, which needs to use the C8y HTTP
//! proxy to report on their progress. Additionally, while executing operations we often need to
//! send messages to different actors and wait for their results before continuing.
//!
//! The operations are always triggered remotely by Cumulocity, and a triggered operation must
//! always terminate in a success or failure. This status needs to be reported to Cumulocity.
//!
//! This module contains:
//! - data definitions of various states which are necessary to maintain in the mapper
//! - status and error handing utilities for reporting operation success/failure in different ways
//!   (MQTT, Smartrest)
//! - implementations of operations

use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::Capabilities;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use camino::Utf8Path;
use std::sync::Arc;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::commands::ConfigMetadata;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::Jsonify;
use tedge_config::AutoLogUpload;
use tedge_config::SoftwareManagementApiFlag;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tracing::error;

pub mod config_snapshot;
pub mod config_update;
pub mod firmware_update;
pub mod log_upload;
mod restart;
mod software_list;
mod software_update;
mod upload;

/// Handles operations.
///
/// Handling an operation usually consists of 3 steps:
///
/// 1. Receive a smartrest message which is an operation request, convert it to thin-edge message,
///    and publish on local MQTT (done by the converter).
/// 2. Various local thin-edge components/services execute the operation, and when they're done,
///    they publish an MQTT message with 'status: successful/failed'
/// 3. The cumulocity mapper needs to do some additional steps, like downloading/uploading files via
///    HTTP, or talking to C8y via HTTP proxy, before it can send operation response via the bridge
///    and then clear the local MQTT operation topic.
///
/// This struct concerns itself with performing step 3. We need to handle multiple operations
/// concurrently, so we need an object separate from [`CumulocityConverter`], where many `&mut self`
/// methods prevent us from concurrent data access. Instead, this object has all necessary data for
/// operation handling, and `&self` methods so it can be run concurrently.
#[derive(Clone)]
pub struct OperationHandler {
    pub capabilities: Capabilities,
    pub auto_log_upload: AutoLogUpload,
    pub tedge_http_host: Arc<str>,
    pub tmp_dir: Arc<Utf8Path>,
    pub mqtt_schema: MqttSchema,
    pub c8y_prefix: tedge_config::TopicPrefix,
    pub software_management_api: SoftwareManagementApiFlag,
    pub command_id: IdGenerator,

    pub http_proxy: C8YHttpProxy,
    pub c8y_endpoint: C8yEndPoint,
    pub auth_proxy: ProxyUrlGenerator,

    pub downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    pub uploader: ClientMessageBox<IdUploadRequest, IdUploadResult>,
    pub mqtt_publisher: LoggingSender<MqttMessage>,
}

impl OperationHandler {
    pub async fn handle_operation(
        self: &Arc<Self>,
        operation: OperationType,
        entity: Entity,
        cmd_id: Arc<str>,
        message: MqttMessage,
    ) {
        let handler = self.clone();
        let external_id = entity.external_id.clone();
        tokio::spawn(async move {
            let res = match operation {
                OperationType::Health | OperationType::Custom(_) => Ok((vec![], None)),

                OperationType::Restart => {
                    handler
                        .publish_restart_operation_status(entity, &cmd_id, message)
                        .await
                }
                OperationType::SoftwareList => {
                    handler
                        .publish_software_list(entity, &cmd_id, &message)
                        .await
                }
                OperationType::SoftwareUpdate => {
                    handler
                        .publish_software_update_status(entity, &cmd_id, &message)
                        .await
                }
                OperationType::LogUpload => {
                    handler
                        .handle_log_upload_state_change(entity, &cmd_id, &message)
                        .await
                }
                OperationType::ConfigSnapshot => {
                    handler
                        .handle_config_snapshot_state_change(entity, &cmd_id, &message)
                        .await
                }
                OperationType::ConfigUpdate => {
                    handler
                        .handle_config_update_state_change(entity, &cmd_id, &message)
                        .await
                }
                OperationType::FirmwareUpdate => {
                    handler
                        .handle_firmware_update_state_change(entity, &cmd_id, &message)
                        .await
                }
            };

            let mut mqtt_publisher = handler.mqtt_publisher.clone();
            match res {
                // If there are mapped final status messages to be published, they are cached until the operation log is uploaded
                Ok((messages, Some(command))) if !messages.is_empty() => {
                    if let Err(e) = handler
                        .upload_operation_log(&external_id, &cmd_id, &operation, command)
                        .await
                    {
                        error!("failed to upload operation logs: {e}");
                    }

                    for message in messages {
                        mqtt_publisher.send(message).await.unwrap();
                    }
                }
                Ok((messages, _)) => {
                    for message in messages {
                        mqtt_publisher.send(message).await.unwrap();
                    }
                }
                Err(e) => error!("{e}"),
            }
        });
    }
}

/// A subset of entity-related information necessary to handle an operation.
///
/// Because the operation may take time and other operations may run concurrently, we don't want to
/// query the entity store.
#[derive(Clone, Debug)]
pub struct Entity {
    pub topic_id: EntityTopicId,
    pub external_id: EntityExternalId,
    pub smartrest_publish_topic: Topic,
}

impl CumulocityConverter {
    fn convert_config_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
        c8y_op_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let metadata = ConfigMetadata::from_json(message.payload_str()?)?;

        let mut messages = match self.register_operation(topic_id, c8y_op_name) {
            Err(err) => {
                error!("Failed to register {c8y_op_name} operation for {topic_id} due to: {err}");
                return Ok(vec![]);
            }
            Ok(messages) => messages,
        };

        // To SmartREST supported config types
        let mut types = metadata.types;
        types.sort();
        let supported_config_types = types.join(",");
        let payload = format!("119,{supported_config_types}");
        let sm_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        messages.push(MqttMessage::new(&sm_topic, payload));

        Ok(messages)
    }
}

fn get_smartrest_response_for_upload_result(
    upload_result: tedge_uploader_ext::UploadResult,
    binary_url: &str,
    operation: c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations,
) -> c8y_api::smartrest::smartrest_serializer::SmartRest {
    match upload_result {
        Ok(_) => c8y_api::smartrest::smartrest_serializer::succeed_static_operation(
            operation,
            Some(binary_url),
        ),
        Err(err) => c8y_api::smartrest::smartrest_serializer::fail_operation(
            operation,
            &format!("Upload failed with {err}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::TestHandle;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn mapper_converts_config_metadata_to_supported_op_and_types_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                ("c8y/s/us", "114,c8y_UploadConfigFile"),
                ("c8y/s/us", "119,typeA,typeB,typeC"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_UploadConfigFile")
            .exists());

        // Simulate config_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update"),
            r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us",
                    "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
                ),
                ("c8y/s/us", "119,typeD,typeE,typeF"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_DownloadConfigFile")
            .exists());
    }

    #[tokio::test]
    async fn mapper_converts_config_cmd_to_supported_op_and_types_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        mqtt.skip(2).await; // Skip the mapped child device registration message

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_UploadConfigFile",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "119,typeA,typeB,typeC",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_UploadConfigFile")
            .exists());

        // Sending an updated list of config types
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
            r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Assert that the updated config type list does not trigger a duplicate supported ops message
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "119,typeB,typeC,typeD",
            )],
        )
        .await;

        // Simulate config_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update"),
            r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "119,typeD,typeE,typeF",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_DownloadConfigFile")
            .exists());

        // Sending an updated list of config types
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update"),
            r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Assert that the updated config type list does not trigger a duplicate supported ops message
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "119,typeB,typeC,typeD",
            )],
        )
        .await;
    }
}
