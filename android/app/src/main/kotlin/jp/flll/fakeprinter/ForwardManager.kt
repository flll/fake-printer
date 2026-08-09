package jp.flll.fakeprinter

import java.io.File

/** 未転送ファイル名の永続化（Android 実装は SharedPreferences、テストはメモリ） */
interface PendingStore {
    fun load(): Set<String>
    fun save(names: Set<String>)
}

/**
 * 受信 PDF を Windows 版 fake-printer へ自動転送する。
 * 送信失敗分は pending として保持し、次の enqueue / flush で再試行する。
 * 純 JVM（Android 非依存）— 単体テスト可能。
 */
class ForwardManager(
    private val dir: File,
    private val store: PendingStore,
    private val sender: (File) -> Boolean,
    private val log: (String) -> Unit = {},
) {
    private val lock = Any()

    fun enqueue(file: File) {
        synchronized(lock) { store.save(store.load() + file.name) }
        flush()
    }

    fun flush() {
        synchronized(lock) {
            val pending = store.load().toMutableSet()
            if (pending.isEmpty()) return
            for (name in pending.toList()) {
                val file = File(dir, name)
                when {
                    !file.exists() -> {
                        pending.remove(name)
                        log("forward: $name は消えているためキューから除去")
                    }
                    runCatching { sender(file) }.getOrDefault(false) -> {
                        pending.remove(name)
                        log("forward: $name を転送済み")
                    }
                    else -> log("forward: $name の転送に失敗、キューに保持")
                }
            }
            store.save(pending)
        }
    }

    fun isPending(name: String): Boolean = synchronized(lock) { name in store.load() }
}
