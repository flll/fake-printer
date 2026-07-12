package jp.flll.fakeprinter

import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.net.HttpURLConnection
import java.net.URL

/**
 * Minimal IPP 2.0 Print-Job client for the fake-printer server.
 * Pure JVM (no Android imports) so it can be unit-tested on the desktop.
 */
object IppClient {

    class Result(val httpStatus: Int, val ippStatus: Int, val jobId: Int?, val jobState: Int?) {
        val isSuccess: Boolean get() = httpStatus == 200 && ippStatus < 0x0100
    }

    private const val OP_PRINT_JOB = 0x0002
    private const val TAG_OPERATION_ATTRS = 0x01
    private const val TAG_JOB_ATTRS = 0x02
    private const val TAG_END = 0x03
    private val DELIMITER_TAGS = setOf(0x01, 0x02, 0x03, 0x04, 0x05)

    private const val VT_INTEGER = 0x21
    private const val VT_ENUM = 0x23
    private const val VT_NAME = 0x42
    private const val VT_URI = 0x45
    private const val VT_CHARSET = 0x47
    private const val VT_NATURAL_LANGUAGE = 0x48
    private const val VT_MIME_TYPE = 0x49

    fun printJob(
        host: String,
        port: Int,
        path: String,
        jobName: String,
        userName: String,
        document: ByteArray,
        requestId: Int = 1,
        connectTimeoutMs: Int = 5000,
        readTimeoutMs: Int = 60000,
    ): Result {
        val header = buildPrintJobHeader(host, port, path, jobName, userName, requestId)

        val url = URL("http", host, port, path)
        val conn = url.openConnection() as HttpURLConnection
        try {
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.connectTimeout = connectTimeoutMs
            conn.readTimeout = readTimeoutMs
            conn.setRequestProperty("Content-Type", "application/ipp")
            conn.setFixedLengthStreamingMode(header.size.toLong() + document.size.toLong())
            conn.outputStream.use { out ->
                out.write(header)
                out.write(document)
            }
            val httpStatus = conn.responseCode
            val body = (if (httpStatus in 200..299) conn.inputStream else conn.errorStream)
                ?.use { it.readBytes() } ?: ByteArray(0)
            return parseResponse(httpStatus, body)
        } finally {
            conn.disconnect()
        }
    }

    private fun buildPrintJobHeader(
        host: String,
        port: Int,
        path: String,
        jobName: String,
        userName: String,
        requestId: Int,
    ): ByteArray {
        val buf = ByteArrayOutputStream()
        val out = DataOutputStream(buf)
        out.writeByte(2) // IPP 2.0
        out.writeByte(0)
        out.writeShort(OP_PRINT_JOB)
        out.writeInt(requestId)
        out.writeByte(TAG_OPERATION_ATTRS)
        writeAttr(out, VT_CHARSET, "attributes-charset", "utf-8")
        writeAttr(out, VT_NATURAL_LANGUAGE, "attributes-natural-language", "en")
        writeAttr(out, VT_URI, "printer-uri", "ipp://$host:$port$path")
        writeAttr(out, VT_NAME, "requesting-user-name", userName)
        writeAttr(out, VT_NAME, "job-name", jobName)
        writeAttr(out, VT_MIME_TYPE, "document-format", "application/pdf")
        out.writeByte(TAG_END)
        out.flush()
        return buf.toByteArray()
    }

    private fun writeAttr(out: DataOutputStream, tag: Int, name: String, value: String) {
        val nameBytes = name.toByteArray(Charsets.UTF_8)
        val valueBytes = value.toByteArray(Charsets.UTF_8)
        out.writeByte(tag)
        out.writeShort(nameBytes.size)
        out.write(nameBytes)
        out.writeShort(valueBytes.size)
        out.write(valueBytes)
    }

    private fun parseResponse(httpStatus: Int, raw: ByteArray): Result {
        if (raw.size < 9) return Result(httpStatus, -1, null, null)
        val ippStatus = ((raw[2].toInt() and 0xFF) shl 8) or (raw[3].toInt() and 0xFF)
        var jobId: Int? = null
        var jobState: Int? = null

        var pos = 8
        var lastName = ""
        while (pos < raw.size) {
            val tag = raw[pos].toInt() and 0xFF
            pos += 1
            if (tag in DELIMITER_TAGS) {
                if (tag == TAG_END) break
                continue
            }
            if (pos + 2 > raw.size) break
            val nameLen = readU16(raw, pos); pos += 2
            if (pos + nameLen > raw.size) break
            val name = if (nameLen > 0) String(raw, pos, nameLen, Charsets.UTF_8) else lastName
            pos += nameLen
            lastName = name
            if (pos + 2 > raw.size) break
            val valueLen = readU16(raw, pos); pos += 2
            if (pos + valueLen > raw.size) break
            if (valueLen == 4 && (tag == VT_INTEGER || tag == VT_ENUM)) {
                val v = ((raw[pos].toInt() and 0xFF) shl 24) or
                        ((raw[pos + 1].toInt() and 0xFF) shl 16) or
                        ((raw[pos + 2].toInt() and 0xFF) shl 8) or
                        (raw[pos + 3].toInt() and 0xFF)
                when (name) {
                    "job-id" -> jobId = v
                    "job-state" -> jobState = v
                }
            }
            pos += valueLen
        }
        return Result(httpStatus, ippStatus, jobId, jobState)
    }

    private fun readU16(raw: ByteArray, pos: Int): Int =
        ((raw[pos].toInt() and 0xFF) shl 8) or (raw[pos + 1].toInt() and 0xFF)
}
