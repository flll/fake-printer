package jp.flll.fakeprinter

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ForwardManagerTest {

    @Rule @JvmField
    val tmp = TemporaryFolder()

    private class MemoryStore : PendingStore {
        private var names: Set<String> = emptySet()
        override fun load(): Set<String> = names
        override fun save(names: Set<String>) { this.names = names.toSet() }
    }

    @Test
    fun failedSendStaysPendingThenRetrySucceeds() {
        val store = MemoryStore()
        var online = false
        val sent = mutableListOf<String>()
        val manager = ForwardManager(tmp.root, store, { file ->
            if (online) { sent.add(file.name); true } else false
        })

        tmp.newFile("doc.pdf")
        manager.enqueue(java.io.File(tmp.root, "doc.pdf"))
        assertTrue("送信失敗分は pending に残る", manager.isPending("doc.pdf"))
        assertEquals(emptyList<String>(), sent)

        online = true
        manager.flush()
        assertFalse("再試行成功で pending から消える", manager.isPending("doc.pdf"))
        assertEquals(listOf("doc.pdf"), sent)
    }

    @Test
    fun successfulSendClearsImmediately() {
        val store = MemoryStore()
        val manager = ForwardManager(tmp.root, store, { true })
        tmp.newFile("ok.pdf")
        manager.enqueue(java.io.File(tmp.root, "ok.pdf"))
        assertFalse(manager.isPending("ok.pdf"))
    }

    @Test
    fun vanishedFileIsDroppedFromQueue() {
        val store = MemoryStore().apply { save(setOf("ghost.pdf")) }
        val manager = ForwardManager(tmp.root, store, { true })
        manager.flush()
        assertFalse("実体が無いファイルはキューから除去", manager.isPending("ghost.pdf"))
    }

    @Test
    fun senderExceptionIsTreatedAsFailure() {
        val store = MemoryStore()
        val manager = ForwardManager(tmp.root, store, { throw RuntimeException("boom") })
        tmp.newFile("err.pdf")
        manager.enqueue(java.io.File(tmp.root, "err.pdf"))
        assertTrue(manager.isPending("err.pdf"))
    }
}
