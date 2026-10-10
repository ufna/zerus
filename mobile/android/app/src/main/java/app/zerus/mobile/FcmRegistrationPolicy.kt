package app.zerus.mobile

object FcmRegistrationPolicy {
    fun needsRegistration(token: String, registered: String, provider: String) =
        token.isNotBlank() && token != registered && provider != "unifiedpush"
    fun retry(status: Int) = status == 408 || status == 429 || status >= 500
}
