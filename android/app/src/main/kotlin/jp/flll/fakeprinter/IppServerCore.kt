package jp.flll.fakeprinter

import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.net.ServerSocket
import java.net.Socket
import java.net.SocketException
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.Executors

/**
 * Minimal IPP/1.1-2.0 print server (the Android port of fake-printer's server.py).
 * Pure JVM — no Android imports — so the whole protocol path is unit-testable.
 *
 * Lessons inherited from the Windows server:
 *  - Print-Job responses MUST carry attributes-charset + job-id/job-uri/job-state
 *    (RFC 8011); Android's Mopria client resends jobs without them.
 *  - Get-Printer-Attributes must report printer-state=processing during/briefly
 *    after a job (busy grace) or Mopria waits ~46s per job for the transition.
 *  - Clients send Expect: 100-continue and chunked bodies; both must be handled.
 */
class IppServerCore(
    port: Int,
    private val printerName: String,
    private val saveDir: File,
    private val ippPath: String = "/ipp/print",
    private val busyGraceMs: Long = 4000,
    private val maxBodyBytes: Int = 100 * 1024 * 1024,
    private val log: (String) -> Unit = {},
) {
    val serverSocket: ServerSocket = ServerSocket(port)
    val localPort: Int get() = serverSocket.localPort

    private val pool = Executors.newCachedThreadPool()
    @Volatile private var running = false

    // --- job registry / busy state (mirrors server.py) ---
    private val lock = Any()
    private var nextJobId = 1
    private val jobStates = LinkedHashMap<Int, Int>() // job-id -> IPP job-state
    private var busyDepth = 0
    private var busyGraceUntil = 0L

    /** Called after a document is stored (for UI refresh hooks). */
    var onDocumentSaved: ((File) -> Unit)? = null

    fun start() {
        running = true
        saveDir.mkdirs()
        pool.execute {
            while (running) {
                val socket = try {
                    serverSocket.accept()
                } catch (_: SocketException) {
                    break // closed by stop()
                }
                pool.execute { handleConnection(socket) }
            }
        }
        log("listening on port $localPort$ippPath")
    }

    fun stop() {
        running = false
        try { serverSocket.close() } catch (_: IOException) {}
        pool.shutdownNow()
    }

    // ------------------------------------------------------------ HTTP layer

    private fun handleConnection(socket: Socket) {
        try {
            socket.soTimeout = 60_000
            val input = BufferedInputStream(socket.getInputStream())
            val output = BufferedOutputStream(socket.getOutputStream())
            while (running) {
                val requestLine = readHttpLine(input) ?: break
                if (requestLine.isBlank()) continue
                val parts = requestLine.split(" ")
                if (parts.size < 3) break
                val (method, target, version) = parts

                val headers = mutableMapOf<String, String>()
                while (true) {
                    val line = readHttpLine(input) ?: return
                    if (line.isEmpty()) break
                    val idx = line.indexOf(':')
                    if (idx > 0) headers[line.substring(0, idx).trim().lowercase()] =
                        line.substring(idx + 1).trim()
                }

                if (headers["expect"]?.lowercase() == "100-continue") {
                    output.write("HTTP/1.1 100 Continue\r\n\r\n".toByteArray())
                    output.flush()
                }

                val body: ByteArray = when {
                    headers["transfer-encoding"]?.contains("chunked", ignoreCase = true) == true ->
                        readChunkedBody(input)
                    headers["content-length"] != null -> {
                        val len = headers["content-length"]!!.toLongOrNull() ?: -1L
                        if (len < 0 || len > maxBodyBytes) {
                            respond(output, 413, "text/plain", "too large".toByteArray()); break
                        }
                        readExact(input, len.toInt())
                    }
                    else -> ByteArray(0)
                }

                val pathOnly = target.substringBefore('?')
                when {
                    method == "GET" && (pathOnly == "/healthz" || pathOnly == "/health") ->
                        respond(output, 200, "text/plain; charset=utf-8", "ok\n".toByteArray())
                    method == "POST" && (pathOnly == ippPath || pathOnly.startsWith("$ippPath/")) -> {
                        val ipp = try {
                            handleIpp(body, headers["host"] ?: "127.0.0.1:$localPort")
                        } catch (e: Exception) {
                            log("IPP error: $e")
                            respond(output, 400, "text/plain", "bad ipp request".toByteArray())
                            break
                        }
                        respond(output, 200, "application/ipp", ipp)
                    }
                    else -> respond(output, 404, "text/plain", "not found".toByteArray())
                }

                val connClose = headers["connection"]?.equals("close", ignoreCase = true) == true
                if (connClose || version == "HTTP/1.0") break
            }
        } catch (_: Exception) {
            // per-connection failures must never kill the server
        } finally {
            try { socket.close() } catch (_: IOException) {}
        }
    }

    private fun respond(out: OutputStream, status: Int, contentType: String, body: ByteArray) {
        val reason = when (status) {
            200 -> "OK"; 400 -> "Bad Request"; 404 -> "Not Found"; 413 -> "Payload Too Large"
            else -> "Status"
        }
        out.write(
            ("HTTP/1.1 $status $reason\r\n" +
                "Content-Type: $contentType\r\n" +
                "Content-Length: ${body.size}\r\n" +
                "Server: fake-printer-android/0.2\r\n" +
                "\r\n").toByteArray()
        )
        out.write(body)
        out.flush()
    }

    private fun readHttpLine(input: InputStream): String? {
        val buf = ByteArrayOutputStream()
        while (true) {
            val b = input.read()
            if (b == -1) return if (buf.size() == 0) null else buf.toString("ISO-8859-1")
            if (b == '\n'.code) break
            if (b != '\r'.code) buf.write(b)
            if (buf.size() > 65536) throw IOException("header line too long")
        }
        return buf.toString("ISO-8859-1")
    }

    private fun readExact(input: InputStream, n: Int): ByteArray {
        val out = ByteArray(n)
        var pos = 0
        while (pos < n) {
            val read = input.read(out, pos, n - pos)
            if (read == -1) return out.copyOf(pos) // client closed early; keep what arrived
            pos += read
        }
        return out
    }

    private fun readChunkedBody(input: InputStream): ByteArray {
        val body = ByteArrayOutputStream()
        while (true) {
            val sizeLine = readHttpLine(input) ?: throw IOException("missing chunk size")
            val size = sizeLine.substringBefore(';').trim().toIntOrNull(16)
                ?: throw IOException("bad chunk size")
            if (size == 0) {
                while (true) {
                    val trailer = readHttpLine(input) ?: break
                    if (trailer.isEmpty()) break
                }
                break
            }
            if (body.size() + size > maxBodyBytes) throw IOException("chunked body too large")
            body.write(readExact(input, size))
            readHttpLine(input) // chunk terminator CRLF
        }
        return body.toByteArray()
    }

    // ------------------------------------------------------------- IPP layer

    private object Op {
        const val PRINT_JOB = 0x0002
        const val VALIDATE_JOB = 0x0004
        const val CREATE_JOB = 0x0005
        const val SEND_DOCUMENT = 0x0006
        const val GET_JOB_ATTRIBUTES = 0x0009
        const val GET_JOBS = 0x000A
        const val GET_PRINTER_ATTRIBUTES = 0x000B
    }

    private class IppRequest(raw: ByteArray) {
        val versionMajor: Int
        val versionMinor: Int
        val operationId: Int
        val requestId: Int
        val attrs = mutableMapOf<String, String>()
        val jobIdAttr: Int?
        val document: ByteArray

        init {
            require(raw.size >= 9) { "IPP request too short" }
            versionMajor = raw[0].toInt() and 0xFF
            versionMinor = raw[1].toInt() and 0xFF
            operationId = u16(raw, 2)
            requestId = u32(raw, 4)
            var pos = 8
            var lastName = ""
            var jobId: Int? = null
            loop@ while (pos < raw.size) {
                val tag = raw[pos].toInt() and 0xFF
                pos += 1
                when {
                    tag == 0x03 -> break@loop
                    tag in setOf(0x01, 0x02, 0x04, 0x05) -> continue@loop
                }
                val nameLen = u16(raw, pos); pos += 2
                val name = if (nameLen > 0) String(raw, pos, nameLen, Charsets.UTF_8) else lastName
                pos += nameLen
                lastName = name
                val valueLen = u16(raw, pos); pos += 2
                if (name in setOf("job-name", "document-format", "requesting-user-name", "last-document")) {
                    attrs[name] = String(raw, pos, valueLen, Charsets.UTF_8)
                }
                if (name == "job-id" && valueLen == 4) jobId = u32(raw, pos)
                pos += valueLen
            }
            jobIdAttr = jobId
            document = raw.copyOfRange(minOf(pos, raw.size), raw.size)
        }

        companion object {
            fun u16(b: ByteArray, pos: Int) = ((b[pos].toInt() and 0xFF) shl 8) or (b[pos + 1].toInt() and 0xFF)
            fun u32(b: ByteArray, pos: Int) =
                ((b[pos].toInt() and 0xFF) shl 24) or ((b[pos + 1].toInt() and 0xFF) shl 16) or
                    ((b[pos + 2].toInt() and 0xFF) shl 8) or (b[pos + 3].toInt() and 0xFF)
        }
    }

    private fun handleIpp(raw: ByteArray, hostHeader: String): ByteArray {
        val req = IppRequest(raw)
        log("IPP op=0x%04x request-id=%d job-name=%s bytes=%d".format(
            req.operationId, req.requestId, req.attrs["job-name"] ?: "", req.document.size))

        return when (req.operationId) {
            Op.GET_PRINTER_ATTRIBUTES -> respondOk(req) { w ->
                writePrinterAttributes(w, hostHeader)
            }
            Op.VALIDATE_JOB -> respondOk(req) { }
            Op.PRINT_JOB -> handlePrintJob(req, hostHeader)
            Op.CREATE_JOB -> {
                val jobId = allocateJob(3)
                respondOk(req) { w -> writeJobAttributes(w, hostHeader, jobId, 3) }
            }
            Op.SEND_DOCUMENT -> handleSendDocument(req, hostHeader)
            Op.GET_JOB_ATTRIBUTES -> {
                val jobId = req.jobIdAttr ?: 0
                respondOk(req) { w -> writeJobAttributes(w, hostHeader, jobId, jobState(jobId)) }
            }
            Op.GET_JOBS -> respondOk(req) { w ->
                synchronized(lock) { jobStates.toList() }.forEach { (id, state) ->
                    writeJobAttributes(w, hostHeader, id, state)
                }
            }
            else -> buildResponse(req, 0x0501) { } // server-error-operation-not-supported
        }
    }

    private fun handlePrintJob(req: IppRequest, hostHeader: String): ByteArray {
        if (req.document.isEmpty()) {
            log("rejecting Print-Job with empty payload")
            return buildResponse(req, 0x0400) { }
        }
        val jobId = allocateJob(5)
        busyBegin()
        try {
            saveDocument(req, jobId)
            setJobState(jobId, 9)
        } finally {
            busyEnd()
        }
        return respondOk(req) { w -> writeJobAttributes(w, hostHeader, jobId, 9) }
    }

    private fun handleSendDocument(req: IppRequest, hostHeader: String): ByteArray {
        val jobId = req.jobIdAttr ?: allocateJob(5)
        if (req.document.isEmpty()) {
            // macOS may close a job with an empty last-document Send-Document.
            if (req.attrs["last-document"] != null) {
                setJobState(jobId, 9)
                return respondOk(req) { w -> writeJobAttributes(w, hostHeader, jobId, 9) }
            }
            return buildResponse(req, 0x0400) { }
        }
        busyBegin()
        try {
            saveDocument(req, jobId)
            setJobState(jobId, 9)
        } finally {
            busyEnd()
        }
        return respondOk(req) { w -> writeJobAttributes(w, hostHeader, jobId, 9) }
    }

    private fun saveDocument(req: IppRequest, jobId: Int) {
        val stamp = SimpleDateFormat("yyyyMMdd_HHmmss", Locale.US).format(Date())
        val rawName = (req.attrs["job-name"] ?: "print-job").ifBlank { "print-job" }
        val safeName = rawName.replace(Regex("[^A-Za-z0-9._\\-ぁ-んァ-ヶ一-龠ー]"), "_").take(60)
        val ext = if (req.document.size >= 4 &&
            req.document.copyOfRange(0, 4).contentEquals("%PDF".toByteArray())) "pdf" else "bin"
        val file = File(saveDir, "${stamp}_j${jobId}_$safeName.$ext")
        file.writeBytes(req.document)
        log("saved ${file.name} (${req.document.size} bytes)")
        onDocumentSaved?.invoke(file)
    }

    // ------------------------------------------------------- response writing

    private fun respondOk(req: IppRequest, groups: (DataOutputStream) -> Unit): ByteArray =
        buildResponse(req, 0x0000, groups)

    private fun buildResponse(req: IppRequest, status: Int, groups: (DataOutputStream) -> Unit): ByteArray {
        val buf = ByteArrayOutputStream()
        val w = DataOutputStream(buf)
        w.writeByte(req.versionMajor)
        w.writeByte(req.versionMinor)
        w.writeShort(status)
        w.writeInt(req.requestId)
        w.writeByte(0x01) // operation-attributes-tag
        attr(w, 0x47, "attributes-charset", "utf-8")
        attr(w, 0x48, "attributes-natural-language", "en")
        groups(w)
        w.writeByte(0x03) // end-of-attributes
        w.flush()
        return buf.toByteArray()
    }

    private fun writePrinterAttributes(w: DataOutputStream, hostHeader: String) {
        val busy = isBusy()
        w.writeByte(0x04) // printer-attributes-tag
        attr(w, 0x45, "printer-uri-supported", "ipp://$hostHeader$ippPath")
        attr(w, 0x44, "uri-authentication-supported", "none")
        attr(w, 0x44, "uri-security-supported", "none")
        attr(w, 0x42, "printer-name", printerName)
        attr(w, 0x41, "printer-make-and-model", "fake-printer-android")
        attr(w, 0x44, "ipp-versions-supported", "1.1")
        attrEnums(w, "operations-supported", listOf(
            Op.PRINT_JOB, Op.VALIDATE_JOB, Op.CREATE_JOB, Op.SEND_DOCUMENT,
            Op.GET_JOB_ATTRIBUTES, Op.GET_JOBS, Op.GET_PRINTER_ATTRIBUTES,
        ))
        attr(w, 0x47, "charset-configured", "utf-8")
        attr(w, 0x47, "charset-supported", "utf-8")
        attr(w, 0x48, "natural-language-configured", "en")
        attr(w, 0x48, "generated-natural-language-supported", "en")
        attrBool(w, "printer-is-accepting-jobs", true)
        // busy=processing(4): Android's Mopria client needs to observe the
        // processing->idle transition to consider a job finished quickly.
        attrInt(w, 0x23, "printer-state", if (busy) 4 else 3)
        attr(w, 0x44, "printer-state-reasons", "none")
        attrInt(w, 0x21, "queued-job-count", if (busy) 1 else 0)
        attr(w, 0x49, "document-format-default", "application/pdf")
        attr(w, 0x49, "document-format-supported", "application/pdf")
        attr(w, 0x44, "compression-supported", "none")
        attrBool(w, "color-supported", true)
        attrKeywords(w, "print-color-mode-supported", listOf("auto", "color", "monochrome"))
        attr(w, 0x44, "print-color-mode-default", "auto")
    }

    private fun writeJobAttributes(w: DataOutputStream, hostHeader: String, jobId: Int, state: Int) {
        if (jobId <= 0) return
        w.writeByte(0x02) // job-attributes-tag
        attrInt(w, 0x21, "job-id", jobId)
        attr(w, 0x45, "job-uri", "ipp://$hostHeader$ippPath/job/$jobId")
        attrInt(w, 0x23, "job-state", state)
        attr(w, 0x44, "job-state-reasons", "none")
    }

    private fun attr(w: DataOutputStream, tag: Int, name: String, value: String) {
        val n = name.toByteArray(Charsets.UTF_8)
        val v = value.toByteArray(Charsets.UTF_8)
        w.writeByte(tag); w.writeShort(n.size); w.write(n); w.writeShort(v.size); w.write(v)
    }

    private fun attrInt(w: DataOutputStream, tag: Int, name: String, value: Int) {
        val n = name.toByteArray(Charsets.UTF_8)
        w.writeByte(tag); w.writeShort(n.size); w.write(n); w.writeShort(4); w.writeInt(value)
    }

    private fun attrBool(w: DataOutputStream, name: String, value: Boolean) {
        val n = name.toByteArray(Charsets.UTF_8)
        w.writeByte(0x22); w.writeShort(n.size); w.write(n); w.writeShort(1)
        w.writeByte(if (value) 1 else 0)
    }

    private fun attrEnums(w: DataOutputStream, name: String, values: List<Int>) {
        values.forEachIndexed { i, v ->
            val n = (if (i == 0) name else "").toByteArray(Charsets.UTF_8)
            w.writeByte(0x23); w.writeShort(n.size); w.write(n); w.writeShort(4); w.writeInt(v)
        }
    }

    private fun attrKeywords(w: DataOutputStream, name: String, values: List<String>) {
        values.forEachIndexed { i, v ->
            val n = (if (i == 0) name else "").toByteArray(Charsets.UTF_8)
            val vb = v.toByteArray(Charsets.UTF_8)
            w.writeByte(0x44); w.writeShort(n.size); w.write(n); w.writeShort(vb.size); w.write(vb)
        }
    }

    // ------------------------------------------------------------ job / busy

    private fun allocateJob(initialState: Int): Int = synchronized(lock) {
        val id = nextJobId++
        jobStates[id] = initialState
        id
    }

    private fun setJobState(jobId: Int, state: Int) {
        if (jobId <= 0) return
        synchronized(lock) { jobStates[jobId] = state }
    }

    private fun jobState(jobId: Int): Int = synchronized(lock) { jobStates[jobId] ?: 9 }

    private fun busyBegin() = synchronized(lock) { busyDepth += 1 }

    private fun busyEnd() = synchronized(lock) {
        busyDepth = maxOf(0, busyDepth - 1)
        busyGraceUntil = maxOf(busyGraceUntil, System.nanoTime() / 1_000_000 + busyGraceMs)
    }

    private fun isBusy(): Boolean = synchronized(lock) {
        busyDepth > 0 || System.nanoTime() / 1_000_000 < busyGraceUntil
    }
}
