use std::marker::PhantomData;

use api_models::webhooks::ObjectReferenceId;
use common_utils::{errors::CustomResult, ext_traits::ValueExt};
use error_stack::ResultExt;

use crate::{
    core::{
        errors::{self},
        payments::helpers,
    },
    db::{get_and_deserialize_key, StorageInterface},
    routes::AppState,
    types::{self, api, domain, PaymentAddress},
    utils,
};

fn default_webhook_config() -> api::MerchantWebhookConfig {
    std::collections::HashSet::from([
        api::IncomingWebhookEvent::PaymentIntentSuccess,
        api::IncomingWebhookEvent::PaymentIntentFailure,
        api::IncomingWebhookEvent::PaymentIntentProcessing,
        api::IncomingWebhookEvent::PaymentActionRequired,
        api::IncomingWebhookEvent::RefundSuccess,
    ])
}

const IRRELEVANT_PAYMENT_ID_IN_SOURCE_VERIFICATION_FLOW: &str =
    "irrelevant_payment_id_in_source_verification_flow";
const IRRELEVANT_ATTEMPT_ID_IN_SOURCE_VERIFICATION_FLOW: &str =
    "irrelevant_attempt_id_in_source_verification_flow";
const IRRELEVANT_CONNECTOR_REQUEST_REFERENCE_ID_IN_SOURCE_VERIFICATION_FLOW: &str =
    "irrelevant_connector_request_reference_id_in_source_verification_flow";

pub async fn lookup_webhook_event(
    db: &dyn StorageInterface,
    connector_id: &str,
    merchant_id: &str,
    event: &api::IncomingWebhookEvent,
) -> bool {
    let redis_key = format!("whconf_{merchant_id}_{connector_id}");
    let merchant_webhook_config_result =
        get_and_deserialize_key(db, &redis_key, "MerchantWebhookConfig")
            .await
            .map(|h| &h | &default_webhook_config());

    match merchant_webhook_config_result {
        Ok(merchant_webhook_config) => merchant_webhook_config.contains(event),
        Err(..) => {
            //if failed to fetch from redis. fetch from db and populate redis
            db.find_config_by_key(&redis_key)
                .await
                .map(|config| {
                    if let Ok(set) =
                        serde_json::from_str::<api::MerchantWebhookConfig>(&config.config)
                    {
                        &set | &default_webhook_config()
                    } else {
                        default_webhook_config()
                    }
                })
                .unwrap_or_else(|_| default_webhook_config())
                .contains(event)
        }
    }
}

pub async fn construct_webhook_router_data<'a>(
    state: &'a AppState,
    merchant_account: &crate::types::domain::MerchantAccount,
    key_store: &domain::MerchantKeyStore,
    connector_name: &str,
    merchant_secret: &api_models::webhooks::ConnectorWebhookSecrets,
    object_reference_id: &ObjectReferenceId,
    request_details: &api::IncomingWebhookRequestDetails<'_>,
) -> CustomResult<types::VerifyWebhookSourceRouterData, errors::ApiErrorResponse> {
    let profile_id = utils::get_profile_id_using_object_reference_id(
        &*state.store,
        object_reference_id.clone(),
        merchant_account,
        connector_name,
    )
    .await
    .change_context(errors::ApiErrorResponse::InternalServerError)
    .attach_printable("Error while fetching connector_label")?;
    let merchant_connector_account = helpers::get_merchant_connector_account(
        state,
        merchant_account.merchant_id.as_str(),
        None,
        key_store,
        &profile_id,
        connector_name,
    )
    .await?;
    let auth_type: types::ConnectorAuthType = merchant_connector_account
        .get_connector_account_details()
        .parse_value("ConnectorAuthType")
        .change_context(errors::ApiErrorResponse::InternalServerError)?;

    let router_data = types::RouterData {
        flow: PhantomData,
        merchant_id: merchant_account.merchant_id.clone(),
        connector: connector_name.to_string(),
        customer_id: None,
        payment_id: IRRELEVANT_PAYMENT_ID_IN_SOURCE_VERIFICATION_FLOW.to_string(),
        attempt_id: IRRELEVANT_ATTEMPT_ID_IN_SOURCE_VERIFICATION_FLOW.to_string(),
        status: diesel_models::enums::AttemptStatus::default(),
        payment_method: diesel_models::enums::PaymentMethod::default(),
        connector_auth_type: auth_type,
        description: None,
        return_url: None,
        payment_method_id: None,
        address: PaymentAddress::default(),
        auth_type: diesel_models::enums::AuthenticationType::default(),
        connector_meta_data: None,
        amount_captured: None,
        request: types::VerifyWebhookSourceRequestData {
            webhook_headers: request_details.headers.clone(),
            webhook_body: request_details.body.to_vec().clone(),
            merchant_secret: merchant_secret.clone(),
        },
        response: Err(types::ErrorResponse::default()),
        access_token: None,
        session_token: None,
        reference_id: None,
        payment_method_token: None,
        connector_customer: None,
        recurring_mandate_payment_data: None,
        preprocessing_id: None,
        connector_request_reference_id:
            IRRELEVANT_CONNECTOR_REQUEST_REFERENCE_ID_IN_SOURCE_VERIFICATION_FLOW.to_string(),
        #[cfg(feature = "payouts")]
        payout_method_data: None,
        #[cfg(feature = "payouts")]
        quote_id: None,
        test_mode: None,
        payment_method_balance: None,
        connector_api_version: None,
        connector_http_status_code: None,
    };
    Ok(router_data)
}
