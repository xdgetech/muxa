package tech.xdge.muxa.demo

import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.util.zip.CRC32
import java.util.zip.DeflaterOutputStream

/** Encodes PNGs with java.util.zip only, so it runs on Android and the JVM alike. */
internal object Png {
    private val SIGNATURE = byteArrayOf(0x89.toByte(), 'P'.code.toByte(), 'N'.code.toByte(), 'G'.code.toByte(), 13, 10, 26, 10)
    private const val WIDTH = 1024
    private const val HEIGHT = 768
    private const val WHITE = 0xFFFFFF

    /** A screenshot stand-in: [color] (0xRRGGBB), with a white box in the middle. */
    fun screenshot(color: Int): ByteArray = encode(WIDTH, HEIGHT) { x, y ->
        if (x in 312 until 712 && y in 284 until 484) WHITE else color
    }

    private fun encode(width: Int, height: Int, pixel: (Int, Int) -> Int): ByteArray {
        val raw = ByteArrayOutputStream()
        DeflaterOutputStream(raw).use { z ->
            val row = ByteArray(1 + width * 3)
            for (y in 0 until height) {
                // Filter type 0 (none) leads each row.
                row[0] = 0
                for (x in 0 until width) {
                    val rgb = pixel(x, y)
                    row[1 + x * 3] = (rgb shr 16).toByte()
                    row[2 + x * 3] = (rgb shr 8).toByte()
                    row[3 + x * 3] = rgb.toByte()
                }
                z.write(row)
            }
        }
        val header = ByteArrayOutputStream()
        DataOutputStream(header).run {
            writeInt(width)
            writeInt(height)
            writeByte(8) // bits per channel
            writeByte(2) // truecolor RGB
            writeByte(0) // deflate
            writeByte(0) // adaptive filtering
            writeByte(0) // no interlace
        }
        val png = ByteArrayOutputStream()
        png.write(SIGNATURE)
        chunk(png, "IHDR", header.toByteArray())
        chunk(png, "IDAT", raw.toByteArray())
        chunk(png, "IEND", ByteArray(0))
        return png.toByteArray()
    }

    private fun chunk(out: ByteArrayOutputStream, type: String, data: ByteArray) {
        val typeBytes = type.toByteArray(Charsets.US_ASCII)
        val crc = CRC32().apply {
            update(typeBytes)
            update(data)
        }
        DataOutputStream(out).run {
            writeInt(data.size)
            write(typeBytes)
            write(data)
            writeInt(crc.value.toInt())
        }
    }
}
