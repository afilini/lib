plugins {
    kotlin("jvm") version "1.9.21"
}

group = "cc.getportal"
version = "0.1.0"

repositories {
    mavenCentral()
}

dependencies {
    testImplementation("org.jetbrains.kotlin:kotlin-test")

    api(group = "org.slf4j", name = "slf4j-api", version = "2.0.17")
    implementation(group = "org.slf4j", name = "slf4j-simple", version = "2.0.17")
}

tasks.test {
    useJUnitPlatform()
}
kotlin {
    jvmToolchain(8)
}

tasks.jar {
    archiveBaseName.set("portal-sdk")
}
