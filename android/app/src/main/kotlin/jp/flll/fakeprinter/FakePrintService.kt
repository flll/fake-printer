package jp.flll.fakeprinter

import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.printservice.PrintJob
import android.printservice.PrintService
import android.printservice.PrinterDiscoverySession
import android.util.Log
import java.util.concurrent.Executors

class FakePrintService : PrintService() {

    private val executor = Executors.newSingleThreadExecutor()
    private val mainHandler = Handler(Looper.getMainLooper())

    override fun onCreatePrinterDiscoverySession(): PrinterDiscoverySession =
        FakeDiscoverySession(this)

    override fun onRequestCancelPrintJob(printJob: PrintJob) {
        // Jobs finish within seconds; honor the request if it is still pending.
        if (!printJob.isCompleted && !printJob.isFailed) {
            printJob.cancel()
        }
    }

    override fun onPrintJobQueued(printJob: PrintJob) {
        printJob.start()
        val jobName = printJob.document.info?.name?.toString()
            ?.takeIf { it.isNotBlank() } ?: "print-job"
        val data = printJob.document.data
        if (data == null) {
            printJob.fail("ドキュメントデータがありません")
            return
        }
        val prefs = Prefs(this)
        val host = prefs.host
        val port = prefs.port
        val path = prefs.path

        executor.execute {
            try {
                val bytes = ParcelFileDescriptor.AutoCloseInputStream(data).use { it.readBytes() }
                Log.i(TAG, "sending job '$jobName' (${bytes.size} bytes) to $host:$port$path")
                val result = IppClient.printJob(
                    host = host,
                    port = port,
                    path = path,
                    jobName = jobName,
                    userName = Build.MODEL ?: "android",
                    document = bytes,
                )
                Log.i(
                    TAG,
                    "job '$jobName' http=${result.httpStatus} ipp=0x%04x job-id=${result.jobId} job-state=${result.jobState}"
                        .format(result.ippStatus),
                )
                mainHandler.post {
                    if (result.isSuccess) {
                        printJob.complete()
                    } else {
                        printJob.fail("プリンタ応答エラー (http=${result.httpStatus} ipp=0x%04x)".format(result.ippStatus))
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "job '$jobName' failed", e)
                mainHandler.post {
                    printJob.fail("送信失敗: ${e.message ?: e.javaClass.simpleName}")
                }
            }
        }
    }

    override fun onDestroy() {
        executor.shutdown()
        super.onDestroy()
    }

    companion object {
        private const val TAG = "FakePrintService"
    }
}
