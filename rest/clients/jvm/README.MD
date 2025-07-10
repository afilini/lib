# Portal SDK (JVM)

A Kotlin SDK for interacting with the Portal service via WebSocket and REST API.
This library targets the **JVM platform** in general and can be used not only from Kotlin but also from **Java** or any other JVM-based language.

It provides a typed command/response system using sealed classes for safe and clear protocol handling.

---

## Features

* Typed commands modeled as `sealed interface Command<R : ResponseData>`
* Typed responses using `sealed class Response` and `ResponseData`
* WebSocket client with authentication and asynchronous command handling
* JSON serialization/deserialization with Jackson
* JVM-compatible, usable from Kotlin, Java, Scala, Groovy, and other JVM languages

---

## To-Do

* Support for notifications **(In progress)** 

---

## Installation

The library is currently under development and not yet published to any package repository.

In the meantime, you can import the library as a JAR by running the build with `gradlew clean build`.

---

## Usage Example

```kotlin
import cc.getportal.sdk.Portal
import cc.getportal.sdk.command.Command
import cc.getportal.sdk.model.Profile

fun main() {
    val portal = Portal(
        hostAddress = "http://localhost:2000",
        authToken = "authToken",
        nostrKey = "nostrKey"
    )

    // Request new authentication init URL
    portal.sendCommand(Command.NewKeyHandshakeUrl, 
      onError = { error -> println("Error: $error") }) { response ->
        val url = response.url
        val streamId = response.stream_id
        println("Auth Init URL: $url, Stream ID: $streamId")
    }

    // Update profile
    val profile = Profile(
        name = "test",
        displayName = "Test",
        picture = null,
        nip05 = null
    )

    portal.sendCommand(Command.SetProfile(profile), onError = {}) { profileData ->
        val updatedProfile = profileData.profile
        println("Updated Profile: $updatedProfile")
    }

    // Issue a JWT token
    val pubkey = "02eec5685e141a8fc6ee91e3aad0556bdb4f7b8f3c8c8c8c8c8c8c8c8c8c8c8c8"
    val expiresAt = System.currentTimeMillis() / 1000 + 3600 // 1 hour from now
    
    portal.issueJwt(
        target_key = pubkey,
        expiresAt = expiresAt,
        onError = { error -> println("Failed to issue JWT: $error") },
        onSuccess = { token ->
            println("Issued JWT token: $token")
            
            // Verify the JWT token
            portal.verifyJwt(
                public_key = pubkey,
                token = token,
                onError = { error -> println("Failed to verify JWT: $error") },
                onSuccess = { target_key ->
                    println("JWT verification successful")
                    println("Target key: $target_key")
                }
            )
        }
    )
}
```

---

## Contributing

Feel free to open issues or pull requests for bugs, features, or improvements.
