package jp.flll.fakeprinter

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.net.HttpURLConnection
import java.net.URL

class IppServerCoreTest {

    @Rule @JvmField
    val tmp = TemporaryFolder()

    private lateinit var server: IppServerCore
    private var port = 0

    @Before
    fun startServer() {
        server = IppServerCore(
            port = 0, // ephemeral
            printerName = "Test Printer",
            saveDir = tmp.root,
            busyGraceMs = 400,
        )
        server.start()
        port = server.localPort
    }

    @After
    fun stopServer() {
        server.stop()
    }

    @Test
    fun printJobSavesPdfAndReturnsJobAttributes() {
        val result = IppClient.printJob(
            host = "127.0.0.1", port = port, path = "/ipp/print",
            jobName = "テスト文書", userName = "junit", document = minimalPdf(),
        )
        assertEquals(200, result.httpStatus)
        assertEquals(0, result.ippStatus)
        assertTrue("job-id missing", (result.jobId ?: 0) > 0)
        assertEquals(9, result.jobState)

        val saved = tmp.root.listFiles()!!.single()
        assertTrue("expected .pdf, got ${saved.name}", saved.name.endsWith(".pdf"))
        assertTrue(saved.readBytes().copyOfRange(0, 4).contentEquals("%PDF".toByteArray()))

        // Busy transition: processing(4) within grace, idle(3) afterwards.
        assertEquals(4, printerState())
        Thread.sleep(700)
        assertEquals(3, printerState())
    }

    @Test
    fun chunkedPrintJobIsAccepted() {
        // Real Android/iOS clients stream jobs with Transfer-Encoding: chunked.
        val body = ByteArrayOutputStream().apply {
            write(printJobHeader("chunked-test"))
            write(minimalPdf())
        }.toByteArray()

        val conn = URL("http://127.0.0.1:$port/ipp/print").openConnection() as HttpURLConnection
        conn.requestMethod = "POST"
        conn.doOutput = true
        conn.setRequestProperty("Content-Type", "application/ipp")
        conn.setChunkedStreamingMode(512)
        conn.outputStream.use { it.write(body) }
        val response = conn.inputStream.use { it.readBytes() }
        conn.disconnect()

        val ippStatus = ((response[2].toInt() and 0xFF) shl 8) or (response[3].toInt() and 0xFF)
        assertEquals(0, ippStatus)
        assertEquals(1, tmp.root.listFiles()!!.size)
    }

    @Test
    fun emptyPrintJobIsRejected() {
        val result = IppClient.printJob(
            host = "127.0.0.1", port = port, path = "/ipp/print",
            jobName = "empty", userName = "junit", document = ByteArray(0),
        )
        assertEquals(200, result.httpStatus)
        assertEquals(0x0400, result.ippStatus) // client-error-bad-request
        assertEquals(0, tmp.root.listFiles()!!.size)
    }

    // ------------------------------------------------------------ helpers

    /** Sends Get-Printer-Attributes and returns the printer-state enum value. */
    private fun printerState(): Int {
        val request = ByteArrayOutputStream().also { buf ->
            val w = DataOutputStream(buf)
            w.writeByte(2); w.writeByte(0)
            w.writeShort(0x000B) // Get-Printer-Attributes
            w.writeInt(77)
            w.writeByte(0x01)
            attr(w, 0x47, "attributes-charset", "utf-8")
            attr(w, 0x48, "attributes-natural-language", "en")
            attr(w, 0x45, "printer-uri", "ipp://127.0.0.1:$port/ipp/print")
            w.writeByte(0x03)
        }.toByteArray()

        val conn = URL("http://127.0.0.1:$port/ipp/print").openConnection() as HttpURLConnection
        conn.requestMethod = "POST"
        conn.doOutput = true
        conn.setRequestProperty("Content-Type", "application/ipp")
        conn.outputStream.use { it.write(request) }
        val raw = conn.inputStream.use { it.readBytes() }
        conn.disconnect()

        var pos = 8
        var lastName = ""
        while (pos < raw.size) {
            val tag = raw[pos].toInt() and 0xFF
            pos += 1
            if (tag == 0x03) break
            if (tag in setOf(0x01, 0x02, 0x04, 0x05)) continue
            val nameLen = ((raw[pos].toInt() and 0xFF) shl 8) or (raw[pos + 1].toInt() and 0xFF); pos += 2
            val name = if (nameLen > 0) String(raw, pos, nameLen, Charsets.UTF_8) else lastName
            pos += nameLen; lastName = name
            val valueLen = ((raw[pos].toInt() and 0xFF) shl 8) or (raw[pos + 1].toInt() and 0xFF); pos += 2
            if (name == "printer-state" && valueLen == 4) {
                return ((raw[pos].toInt() and 0xFF) shl 24) or ((raw[pos + 1].toInt() and 0xFF) shl 16) or
                    ((raw[pos + 2].toInt() and 0xFF) shl 8) or (raw[pos + 3].toInt() and 0xFF)
            }
            pos += valueLen
        }
        throw AssertionError("printer-state not found in response")
    }

    private fun printJobHeader(jobName: String): ByteArray =
        ByteArrayOutputStream().also { buf ->
            val w = DataOutputStream(buf)
            w.writeByte(2); w.writeByte(0)
            w.writeShort(0x0002) // Print-Job
            w.writeInt(42)
            w.writeByte(0x01)
            attr(w, 0x47, "attributes-charset", "utf-8")
            attr(w, 0x48, "attributes-natural-language", "en")
            attr(w, 0x45, "printer-uri", "ipp://127.0.0.1:$port/ipp/print")
            attr(w, 0x42, "job-name", jobName)
            attr(w, 0x49, "document-format", "application/pdf")
            w.writeByte(0x03)
        }.toByteArray()

    private fun attr(w: DataOutputStream, tag: Int, name: String, value: String) {
        val n = name.toByteArray(Charsets.UTF_8)
        val v = value.toByteArray(Charsets.UTF_8)
        w.writeByte(tag); w.writeShort(n.size); w.write(n); w.writeShort(v.size); w.write(v)
    }

    /** Tiny but structurally valid single-page PDF with a correct xref table. */
    private fun minimalPdf(): ByteArray {
        val objects = listOf(
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>\nendobj\n",
        )
        val body = StringBuilder("%PDF-1.4\n")
        val offsets = mutableListOf<Int>()
        for (obj in objects) {
            offsets.add(body.length)
            body.append(obj)
        }
        val xrefPos = body.length
        body.append("xref\n0 ${objects.size + 1}\n0000000000 65535 f \n")
        for (off in offsets) body.append("%010d 00000 n \n".format(off))
        body.append("trailer\n<< /Size ${objects.size + 1} /Root 1 0 R >>\nstartxref\n$xrefPos\n%%EOF\n")
        return body.toString().toByteArray(Charsets.US_ASCII)
    }
}
