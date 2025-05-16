package cc.getportal.sdk

import org.slf4j.LoggerFactory


class Portal(val authToken : String, val nostrKey : String) {

    init {

    }

    companion object {
        private val logger = LoggerFactory.getLogger(Portal::class.java)
    }
}