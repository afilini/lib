package cc.getportal.sdk.command

import com.fasterxml.jackson.annotation.*
import cc.getportal.sdk.model.Profile

// ---------- CommandWithId wrapper ----------
data class CommandWithId(
    val id: String,
    val params: Command<*>
)


// ---------- Command base ----------
@JsonTypeInfo(use = JsonTypeInfo.Id.NAME, include = JsonTypeInfo.As.PROPERTY, property = "cmd")
@JsonSubTypes(
    JsonSubTypes.Type(Command.Auth::class, name = "Auth"),
    JsonSubTypes.Type(Command.NewKeyHandshakeUrl::class, name = "NewKeyHandshakeUrl"),
    JsonSubTypes.Type(Command.AuthenticateKey::class, name = "AuthenticateKey"),
    JsonSubTypes.Type(Command.RequestRecurringPayment::class, name = "RequestRecurringPayment"),
    JsonSubTypes.Type(Command.RequestSinglePayment::class, name = "RequestSinglePayment"),
    JsonSubTypes.Type(Command.RequestPaymentRaw::class, name = "RequestPaymentRaw"),
    JsonSubTypes.Type(Command.FetchProfile::class, name = "FetchProfile"),
    JsonSubTypes.Type(Command.SetProfile::class, name = "SetProfile"),
    JsonSubTypes.Type(Command.IssueJwt::class, name = "IssueJwt"),
    JsonSubTypes.Type(Command.VerifyJwt::class, name = "VerifyJwt")
)
sealed interface Command<R : ResponseData> {
    // --- Command implementations ---
    data class Auth(val token: String) : Command<ResponseData.AuthSuccess>
    data object NewKeyHandshakeUrl : Command<ResponseData.KeyHandshakeUrl>
    data class AuthenticateKey(val main_key: String, val subkeys: List<String>) : Command<ResponseData.AuthResponse>

    @Deprecated("Not fully implemented")
    data class RequestRecurringPayment(
        val main_key: String,
        val subkeys: List<String>,
        val payment_request: Any // TODO: Replace with actual type
    ) : Command<ResponseData.RecurringPayment>

    @Deprecated("Not fully implemented")
    data class RequestSinglePayment(
        val main_key: String,
        val subkeys: List<String>,
        val payment_request: Any // TODO: Replace with actual type
    ) : Command<ResponseData.SinglePayment>

    @Deprecated("Not fully implemented")
    data class RequestPaymentRaw(
        val main_key: String,
        val subkeys: List<String>,
        val payment_request: Any // TODO: Replace with actual type
    ) : Command<ResponseData.SinglePayment>

    data class FetchProfile(val main_key: String) : Command<ResponseData.ProfileData>
    data class SetProfile(val profile: Profile) : Command<ResponseData.ProfileData>
    data class IssueJwt(val pubkey: String, val expires_at: Long) : Command<ResponseData.IssueJwt>
    data class VerifyJwt(val pubkey: String, val token: String) : Command<ResponseData.VerifyJwt>
}



// ---------- Response sealed class ----------
@JsonTypeInfo(use = JsonTypeInfo.Id.NAME, include = JsonTypeInfo.As.PROPERTY, property = "type")
@JsonSubTypes(
    JsonSubTypes.Type(Response.Success::class, name = "success"),
    JsonSubTypes.Type(Response.Error::class, name = "error"),
    JsonSubTypes.Type(Response.Notification::class, name = "notification")
)
sealed class Response {
    data class Success(val id: String, val data: ResponseData) : Response()
    data class Error(val id: String, val message: String) : Response()
    data class Notification(val id: String, val data: NotificationData) : Response()
}

// ---------- ResponseData sealed class ----------
@JsonTypeInfo(use = JsonTypeInfo.Id.NAME, include = JsonTypeInfo.As.PROPERTY, property = "type")
@JsonSubTypes(
    JsonSubTypes.Type(ResponseData.AuthSuccess::class, name = "auth_success"),
    JsonSubTypes.Type(ResponseData.KeyHandshakeUrl::class, name = "key_handshake_url"),
    JsonSubTypes.Type(ResponseData.AuthResponse::class, name = "auth_response"),
    JsonSubTypes.Type(ResponseData.RecurringPayment::class, name = "recurring_payment"),
    JsonSubTypes.Type(ResponseData.SinglePayment::class, name = "single_payment"),
    JsonSubTypes.Type(ResponseData.ProfileData::class, name = "profile"),
    JsonSubTypes.Type(ResponseData.IssueJwt::class, name = "issue_jwt"),
    JsonSubTypes.Type(ResponseData.VerifyJwt::class, name = "verify_jwt")
)
sealed class ResponseData {
    data class AuthSuccess(val message: String) : ResponseData()
    data class KeyHandshakeUrl(val url: String, val stream_id: String) : ResponseData()
    data class AuthResponse(val event: AuthResponseData) : ResponseData()
    @Deprecated("Not fully implemented")
    data class RecurringPayment(val status: Any) : ResponseData() // TODO: Replace Any
    @Deprecated("Not fully implemented")
    data class SinglePayment(val status: Any, val stream_id: String?) : ResponseData() // TODO: Replace Any
    data class ProfileData(val profile: Profile?) : ResponseData()
    data class IssueJwt(val token: String) : ResponseData()
    data class VerifyJwt(val target_key: String) : ResponseData()
}

data class AuthResponseData(
    val user_key: String,
    val recipient: String,
    val challenge: String,
    val granted_permissions: List<String>,
    val session_token: String
)

// ---------- NotificationData sealed class ----------
sealed class NotificationData {
    data class KeyHandshake(val main_key: String) : NotificationData()
    data class PaymentStatusUpdate(val status: InvoiceStatus) : NotificationData()
}

sealed class InvoiceStatus {
    data class Paid(val preimage: String?) : InvoiceStatus()
    object Timeout : InvoiceStatus()
    data class Error(val reason: String) : InvoiceStatus()
}