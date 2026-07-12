package jp.flll.fakeprinter

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import kotlin.concurrent.thread

/**
 * 受信→転送の統合テスト: Android 側相当の IppServerCore に印刷すると、
 * ForwardManager が Windows 版実サーバ (127.0.0.1:8631) へ自動転送する。
 * Windows 版が稼働していないときは自動スキップ。
 */
class ForwardChainLiveTest {

    @Rule @JvmField
    val tmp = TemporaryFolder()

    private class MemoryStore : PendingStore {
        private var names: Set<String> = emptySet()
        override fun load(): Set<String> = names
        override fun save(names: Set<String>) { this.names = names.toSet() }
    }

    @Test
    fun receiveThenForwardToRealWindowsServer() {
        assumeTrue("Windows 版 fake-printer が 127.0.0.1:8631 で稼働していない", windowsReachable())

        val store = MemoryStore()
        val forwarder = ForwardManager(tmp.root, store, { file ->
            IppClient.printJob(
                host = "127.0.0.1", port = 8631, path = "/ipp/print",
                jobName = file.name, userName = "chain-test", document = file.readBytes(),
            ).isSuccess
        })

        val core = IppServerCore(port = 0, printerName = "Chain Test", saveDir = tmp.root)
        core.onDocumentSaved = { file -> thread { forwarder.enqueue(file) } } // サービスと同じ非同期化
        core.start()
        try {
            val result = IppClient.printJob(
                host = "127.0.0.1", port = core.localPort, path = "/ipp/print",
                jobName = "chain-live-test", userName = "junit", document = minimalPdf(),
            )
            assertEquals(0, result.ippStatus)

            // 非同期転送の完了を待つ（最大10秒）
            val deadline = System.currentTimeMillis() + 10_000
            while (System.currentTimeMillis() < deadline && store.load().isNotEmpty()) {
                Thread.sleep(200)
            }
            assertTrue("転送キューが10秒以内に空にならなかった", store.load().isEmpty())
        } finally {
            core.stop()
        }
    }

    private fun windowsReachable(): Boolean = try {
        val conn = URL("http://127.0.0.1:8631/healthz").openConnection() as HttpURLConnection
        conn.connectTimeout = 1500
        conn.readTimeout = 1500
        val ok = conn.responseCode == 200
        conn.disconnect()
        ok
    } catch (_: Exception) {
        false
    }

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
