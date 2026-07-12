package jp.flll.fakeprinter

import android.print.PrintAttributes
import android.print.PrinterCapabilitiesInfo
import android.print.PrinterId
import android.print.PrinterInfo
import android.printservice.PrinterDiscoverySession

class FakeDiscoverySession(private val service: FakePrintService) : PrinterDiscoverySession() {

    override fun onStartPrinterDiscovery(priorityList: MutableList<PrinterId>) {
        publishPrinter()
    }

    private fun publishPrinter() {
        val prefs = Prefs(service)
        val printerId = service.generatePrinterId("fake-printer")
        val capabilities = PrinterCapabilitiesInfo.Builder(printerId)
            .addMediaSize(PrintAttributes.MediaSize.ISO_A4, true)
            .addMediaSize(PrintAttributes.MediaSize.NA_LETTER, false)
            .addResolution(PrintAttributes.Resolution("r200", "200 dpi", 200, 200), true)
            .setColorModes(
                PrintAttributes.COLOR_MODE_COLOR or PrintAttributes.COLOR_MODE_MONOCHROME,
                PrintAttributes.COLOR_MODE_COLOR,
            )
            .setMinMargins(PrintAttributes.Margins.NO_MARGINS)
            .build()
        val printer = PrinterInfo.Builder(printerId, "Fake Printer", PrinterInfo.STATUS_IDLE)
            .setDescription("fake-printer @ ${prefs.host}:${prefs.port}")
            .setCapabilities(capabilities)
            .build()
        addPrinters(listOf(printer))
    }

    override fun onStopPrinterDiscovery() = Unit

    override fun onValidatePrinters(printerIds: MutableList<PrinterId>) = Unit

    override fun onStartPrinterStateTracking(printerId: PrinterId) = Unit

    override fun onStopPrinterStateTracking(printerId: PrinterId) = Unit

    override fun onDestroy() = Unit
}
