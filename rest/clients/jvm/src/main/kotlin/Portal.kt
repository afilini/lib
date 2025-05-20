package cc.getportal.sdk

import cc.getportal.sdk.exception.PortalException
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.slf4j.LoggerFactory
import org.w3c.dom.Text


class Portal(val hostAddress: String, val authToken: String, val nostrKey: String) {

    private val httpClient : OkHttpClient = OkHttpClient()


    private fun getHealth() : Boolean {
        val request = Request.Builder()
            .url("$hostAddress/health")
            .addHeader("Authorization", "Bearer $authToken")
            .build()
        val response = httpClient.newCall(request).execute()

        val body = response.body ?: return false
        return body.string() == "OK"

    }

    private fun connect() {

        try {
            if(!getHealth()) {
                throw PortalException(message = "Server is not running")
            }
            startWsClient()
        } catch (e : Exception) {
            throw PortalException(message = null, throwable = e)
        }
    }

    private fun startWsClient() {
        val request = Request.Builder()
            .url("$hostAddress/ws")
            .addHeader("Authorization", "Bearer $authToken")
            .build()

        val ws = httpClient.newWebSocket(request, object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) {
                super.onMessage(webSocket, text)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                super.onClosed(webSocket, code, reason)
            }
        })
        ws.send("""
            {
              "cmd": "Auth",
              "params": {
                "token": "$authToken"
              }
            }
        """)
    }

    companion object {
        private val logger = LoggerFactory.getLogger(Portal::class.java)
    }
}