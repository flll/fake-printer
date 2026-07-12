package jp.flll.fakeprinter

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.net.HttpURLConnection
import java.net.URL

/**
 * Live test against a locally running fake-printer server (127.0.0.1:8631).
 * Skipped automatically when the server is not running.
 */
class IppClientLiveTest {

    @Test
    fun printJobAgainstLocalServer() {
        assumeTrue("fake-printer is not running on 127.0.0.1:8631", serverReachable())

        val result = IppClient.printJob(
            host = "127.0.0.1",
            port = 8631,
            path = "/ipp/print",
            jobName = "jvm-live-test",
            userName = "jvm-test",
            document = minimalPdf(),
        )

        assertEquals(200, result.httpStatus)
        assertEquals(0, result.ippStatus)
        assertTrue("job-id missing", (result.jobId ?: 0) > 0)
        assertEquals(9, result.jobState)
    }

    private fun serverReachable(): Boolean = try {
        val conn = URL("http://127.0.0.1:8631/healthz").openConnection() as HttpURLConnection
        conn.connectTimeout = 1500
        conn.readTimeout = 1500
        val ok = conn.responseCode == 200
        conn.disconnect()
        ok
    } catch (_: Exception) {
        false
    }

    /** Builds a tiny but structurally valid single-page PDF with a correct xref table. */
    private fun minimalPdf(): ByteArray {
        val header = "%PDF-1.4\n"
        val objects = listOf(
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] >>\nendobj\n",
        )
        val body = StringBuilder(header)
        val offsets = mutableListOf<Int>()
        for (obj in objects) {
            offsets.add(body.length)
            body.append(obj)
        }
        val xrefPos = body.length
        body.append("xref\n0 ${objects.size + 1}\n")
        body.append("0000000000 65535 f \n")
        for (off in offsets) {
            body.append("%010d 00000 n \n".format(off))
        }
        body.append("trailer\n<< /Size ${objects.size + 1} /Root 1 0 R >>\nstartxref\n$xrefPos\n%%EOF\n")
        return body.toString().toByteArray(Charsets.US_ASCII)
    }
}
