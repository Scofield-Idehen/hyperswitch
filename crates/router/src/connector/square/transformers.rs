use error_stack::IntoReport;
use masking::{ExposeInterface, PeekInterface, Secret};
use serde::{Deserialize, Serialize};

use crate::{
    connector::utils::{CardData, PaymentsAuthorizeRequestData, RouterData},
    core::errors,
    types::{self, api, storage::enums},
};

//Should i make exp year and exp month secret?
#[derive(Debug, Serialize)]
pub struct SquareCardData {
    cvv: Secret<String>,
    exp_month: u16,
    exp_year: u16,
    number: cards::CardNumber,
}
#[derive(Debug, Serialize)]
pub struct SquareTokenizeData {
    client_id: Secret<String>,
    session_id: Secret<String>,
    card_data: SquareCardData,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum SquareTokenRequest {
    Card(SquareTokenizeData),
}

impl TryFrom<&types::TokenizationRouterData> for SquareTokenRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &types::TokenizationRouterData) -> Result<Self, Self::Error> {
        // Handle this error properly
        let client_id = match &item.connector_auth_type {
            types::ConnectorAuthType::BodyKey { key1, .. } => key1.to_owned(),
            _ => Secret::new("".to_string()),
        };
        match item.request.payment_method_data.clone() {
            api::PaymentMethodData::Card(card_data) => {
                let square_card_data = SquareTokenizeData {
                    client_id,
                    session_id: Secret::new("ADD_SESSION_ID_HERE".to_string()),
                    card_data: SquareCardData {
                        exp_year: card_data.get_expiry_year_4_digit().peek().parse().unwrap(),
                        exp_month: card_data.card_exp_month.peek().parse().unwrap(),
                        number: card_data.card_number,
                        cvv: card_data.card_cvc,
                    },
                };
                Ok(Self::Card(square_card_data))
            }
            api::PaymentMethodData::BankDebit(_)
            | api::PaymentMethodData::Wallet(_)
            | api::PaymentMethodData::PayLater(_)
            | api::PaymentMethodData::BankRedirect(_)
            | api::PaymentMethodData::BankTransfer(_)
            | api::PaymentMethodData::Crypto(_)
            | api::PaymentMethodData::MandatePayment
            | api::PaymentMethodData::Reward(_)
            | api::PaymentMethodData::Voucher(_)
            | api::PaymentMethodData::GiftCard(_)
            | api::PaymentMethodData::Upi(_) => Err(errors::ConnectorError::NotImplemented(
                "Payment Method".to_string(),
            ))
            .into_report(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SquareTokenResponse {
    card_nonce: Secret<String>,
}

impl<F, T>
    TryFrom<types::ResponseRouterData<F, SquareTokenResponse, T, types::PaymentsResponseData>>
    for types::RouterData<F, T, types::PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: types::ResponseRouterData<F, SquareTokenResponse, T, types::PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(types::PaymentsResponseData::TokenizationResponse {
                token: item.response.card_nonce.expose(),
            }),
            ..item.data
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StaxPaymentsAmountData {
    amount: i64,
    currency: enums::Currency,
}
#[derive(Debug, Serialize)]
pub struct StaxPaymentsRequestExternalDetails {
    source: String,
    #[serde(rename = "type")]
    source_type: String,
}
//TODO: Fill the struct with respective fields
#[derive(Debug, Serialize)]
pub struct SquarePaymentsRequest {
    amount_money: StaxPaymentsAmountData,
    idempotency_key: Secret<String>,
    source_id: Secret<String>,
    autocomplete: bool,
    external_details: StaxPaymentsRequestExternalDetails,
}

impl TryFrom<&types::PaymentsAuthorizeRouterData> for SquarePaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &types::PaymentsAuthorizeRouterData) -> Result<Self, Self::Error> {
        let autocomplete = item.request.is_auto_capture()?;
        match item.request.payment_method_data.clone() {
            api::PaymentMethodData::Card(_) => Ok(Self {
                idempotency_key: Secret::new(item.payment_id.clone()),
                source_id: Secret::new(item.get_payment_method_token()?),
                amount_money: StaxPaymentsAmountData {
                    amount: item.request.amount,
                    currency: item.request.currency,
                },
                autocomplete,
                external_details: StaxPaymentsRequestExternalDetails {
                    source: "Hyperswitch".to_string(),
                    source_type: "Card".to_string(),
                },
            }),
            _ => Err(errors::ConnectorError::NotImplemented("Payment methods".to_string()).into()),
        }
    }
}

//TODO: Fill the struct with respective fields
// Auth Struct
pub struct SquareAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&types::ConnectorAuthType> for SquareAuthType {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(auth_type: &types::ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            types::ConnectorAuthType::BodyKey { api_key, .. } => Ok(Self {
                api_key: api_key.to_owned(),
            }),
            _ => Err(errors::ConnectorError::FailedToObtainAuthType.into()),
        }
    }
}
// PaymentsResponse
//TODO: Append the remaining status flags
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SquarePaymentStatus {
    Completed,
    Failed,
    Approved,
    Canceled,
    Pending,
    #[default]
    Processing,
}

impl From<SquarePaymentStatus> for enums::AttemptStatus {
    fn from(item: SquarePaymentStatus) -> Self {
        match item {
            SquarePaymentStatus::Completed => Self::Charged,
            SquarePaymentStatus::Approved => Self::Authorized,
            SquarePaymentStatus::Failed => Self::Failure,
            SquarePaymentStatus::Canceled => Self::Voided,
            SquarePaymentStatus::Processing | SquarePaymentStatus::Pending => Self::Authorizing,
        }
    }
}

//TODO: Fill the struct with respective fields
#[derive(Debug, Deserialize)]
pub struct SquarePaymentsResponseDetails {
    status: SquarePaymentStatus,
    id: String,
    amount_money: StaxPaymentsAmountData,
}
#[derive(Debug, Deserialize)]
pub struct SquarePaymentsResponse {
    payment: SquarePaymentsResponseDetails,
}

impl<F, T>
    TryFrom<types::ResponseRouterData<F, SquarePaymentsResponse, T, types::PaymentsResponseData>>
    for types::RouterData<F, T, types::PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: types::ResponseRouterData<F, SquarePaymentsResponse, T, types::PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            status: enums::AttemptStatus::from(item.response.payment.status),
            response: Ok(types::PaymentsResponseData::TransactionResponse {
                resource_id: types::ResponseId::ConnectorTransactionId(item.response.payment.id),
                redirection_data: None,
                mandate_reference: None,
                connector_metadata: None,
                network_txn_id: None,
                connector_response_reference_id: None,
            }),
            amount_captured: Some(item.response.payment.amount_money.amount),
            ..item.data
        })
    }
}

//TODO: Fill the struct with respective fields
// REFUND :
// Type definition for RefundRequest
#[derive(Debug, Serialize)]
pub struct SquareRefundRequest {
    amount_money: StaxPaymentsAmountData,
    idempotency_key: Secret<String>,
    payment_id: Secret<String>,
}

impl<F> TryFrom<&types::RefundsRouterData<F>> for SquareRefundRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &types::RefundsRouterData<F>) -> Result<Self, Self::Error> {
        Ok(Self {
            amount_money: StaxPaymentsAmountData {
                amount: item.request.refund_amount,
                currency: item.request.currency,
            },
            idempotency_key: Secret::new(item.request.refund_id.clone()),
            payment_id: Secret::new(item.request.connector_transaction_id.clone()),
        })
    }
}

#[derive(Debug, Serialize, Default, Deserialize, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum RefundStatus {
    Completed,
    Failed,
    Pending,
    #[default]
    Processing,
}

impl From<RefundStatus> for enums::RefundStatus {
    fn from(item: RefundStatus) -> Self {
        match item {
            RefundStatus::Completed => Self::Success,
            RefundStatus::Failed => Self::Failure,
            RefundStatus::Processing | RefundStatus::Pending => Self::Pending,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SquareRefundResponseDetails {
    status: RefundStatus,
    id: String,
}
#[derive(Debug, Deserialize)]
pub struct RefundResponse {
    refund: SquareRefundResponseDetails,
}

impl TryFrom<types::RefundsResponseRouterData<api::Execute, RefundResponse>>
    for types::RefundsRouterData<api::Execute>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: types::RefundsResponseRouterData<api::Execute, RefundResponse>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(types::RefundsResponseData {
                connector_refund_id: item.response.refund.id,
                refund_status: enums::RefundStatus::from(item.response.refund.status),
            }),
            ..item.data
        })
    }
}

impl TryFrom<types::RefundsResponseRouterData<api::RSync, RefundResponse>>
    for types::RefundsRouterData<api::RSync>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: types::RefundsResponseRouterData<api::RSync, RefundResponse>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(types::RefundsResponseData {
                connector_refund_id: item.response.refund.id,
                refund_status: enums::RefundStatus::from(item.response.refund.status),
            }),
            ..item.data
        })
    }
}

//TODO: Fill the struct with respective fields
#[derive(Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct SquareErrorResponse {
    pub status_code: u16,
    pub code: String,
    pub message: String,
    pub reason: Option<String>,
}
