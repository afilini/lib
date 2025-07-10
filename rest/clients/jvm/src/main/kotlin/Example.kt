package cc.getportal.sdk

import cc.getportal.sdk.command.ResponseData
import cc.getportal.sdk.exception.PortalException

/**
 * Example usage of the Portal SDK with JWT functionality
 */
fun main() {
    val hostAddress = "ws://localhost:3000"
    val authToken = "your-auth-token" // Replace with your actual token

    try {
        val portal = Portal(
            hostAddress = hostAddress,
            authToken = authToken,
            onClose = { code, reason ->
                println("Connection closed: $code - $reason")
            }
        )

        // Example: Issue a JWT token
        val pubkey = "02eec5685e141a8fc6ee91e3aad0556bdb4f7b8f3c8c8c8c8c8c8c8c8c8c8c8c8"
        val expiresAt = System.currentTimeMillis() / 1000 + 3600 // 1 hour from now

        portal.issueJwt(
            target_key = pubkey,
            expiresAt = expiresAt,
            onError = { error ->
                println("Failed to issue JWT: $error")
            },
            onSuccess = { token ->
                println("Issued JWT token: $token")

                // Example: Verify the JWT token
                portal.verifyJwt(
                    public_key = pubkey,
                    token = token,
                    onError = { error ->
                        println("Failed to verify JWT: $error")
                    },
                    onSuccess = { target_key ->
                        println("JWT verification successful")
                        println("Target key: $target_key")
                    }
                )
            }
        )

        // Keep the application running for a while to see the results
        Thread.sleep(5000)

    } catch (e: PortalException) {
        println("Portal error: ${e.message}")
    } catch (e: Exception) {
        println("Unexpected error: ${e.message}")
        e.printStackTrace()
    }
} 