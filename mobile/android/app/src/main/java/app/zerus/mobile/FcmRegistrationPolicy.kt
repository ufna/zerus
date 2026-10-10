package app.zerus.mobile

object FcmRegistrationPolicy {
    fun needsRegistration(token: String, registered: String) = token.isNotBlank() && token != registered
    fun retry(status: Int) = status == 408 || status == 429 || status >= 500
}
