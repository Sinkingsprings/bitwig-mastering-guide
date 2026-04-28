package com.sinkingsprings.gilligan.ipc;

import com.bitwig.extension.controller.api.ControllerHost;
import java.io.IOException;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.Consumer;

/**
 * Unix domain socket server (Gilligan side of the Skipper↔Gilligan IPC bridge).
 *
 * Framing: 4-byte big-endian message length followed by UTF-8 JSON body.
 *
 * Threading model:
 *   - Controller thread calls setFrame() and drainInbound() (via scheduleTask).
 *   - A single daemon worker thread owns the socket, accepts one client at a
 *     time, sends frames when available, and puts received messages into the
 *     inbound queue for the controller thread to drain.
 */
public class IpcServer {

    private static final int HEADER_BYTES = 4;

    private final ControllerHost host;
    private final Path socketPath;

    /** Latest serialised TrackTable frame — controller thread writes, worker reads. */
    private final AtomicReference<byte[]> latestFrame = new AtomicReference<>(null);

    /** Messages received from Skipper (e.g. FixActions in Phase 12). */
    private final LinkedBlockingQueue<String> inbound = new LinkedBlockingQueue<>();

    private final AtomicBoolean running = new AtomicBoolean(false);
    private Thread workerThread;

    public IpcServer(ControllerHost host) {
        this.host = host;
        String user = System.getProperty("user.name", "user");
        this.socketPath = Path.of("/tmp/skipper-gilligan-" + user + ".sock");
    }

    // ── Controller-thread API ────────────────────────────────────────────────

    public void start() {
        running.set(true);
        workerThread = new Thread(this::workerLoop, "gilligan-ipc");
        workerThread.setDaemon(true);
        workerThread.start();
    }

    public void stop() {
        running.set(false);
        if (workerThread != null) workerThread.interrupt();
    }

    /** Replace the pending frame; the worker sends this to the connected client. */
    public void setFrame(byte[] jsonUtf8) {
        latestFrame.set(jsonUtf8);
    }

    /** Drain messages received from Skipper into the controller thread. */
    public void drainInbound(Consumer<String> handler) {
        String msg;
        while ((msg = inbound.poll()) != null) {
            handler.accept(msg);
        }
    }

    // ── Worker thread ────────────────────────────────────────────────────────

    private void workerLoop() {
        try {
            Files.deleteIfExists(socketPath);

            try (ServerSocketChannel server =
                    ServerSocketChannel.open(StandardProtocolFamily.UNIX)) {

                server.bind(UnixDomainSocketAddress.of(socketPath));
                server.configureBlocking(false);
                host.println("Gilligan IPC listening: " + socketPath);

                while (running.get()) {
                    SocketChannel client = server.accept();
                    if (client != null) {
                        host.println("Gilligan: Skipper connected");
                        serveClient(client);
                        host.println("Gilligan: Skipper disconnected");
                    } else {
                        Thread.sleep(50);
                    }
                }
            }

            Files.deleteIfExists(socketPath);

        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } catch (IOException e) {
            host.println("Gilligan IPC error: " + e.getMessage());
        }
    }

    /**
     * Serve a single connected Skipper client.
     * Sends the latest frame every ~100 ms and reads any inbound data.
     */
    private void serveClient(SocketChannel ch) throws InterruptedException, IOException {
        ch.configureBlocking(false);

        // Reusable read buffer for inbound messages (up to 64 KiB).
        ByteBuffer readBuf = ByteBuffer.allocate(65536);
        ByteBuffer headerBuf = ByteBuffer.allocate(HEADER_BYTES);

        try {
            while (running.get() && ch.isOpen()) {
                // ── Send latest frame if one is available ──────────────────
                byte[] frame = latestFrame.getAndSet(null);
                if (frame != null) {
                    writeMessage(ch, frame);
                }

                // ── Read any inbound data (non-blocking) ───────────────────
                headerBuf.clear();
                int hRead = ch.read(headerBuf);
                if (hRead == -1) break; // client closed
                if (hRead == HEADER_BYTES) {
                    headerBuf.flip();
                    int bodyLen = headerBuf.getInt();
                    if (bodyLen > 0 && bodyLen < readBuf.capacity()) {
                        readBuf.clear();
                        readBuf.limit(bodyLen);
                        while (readBuf.hasRemaining()) {
                            if (ch.read(readBuf) == -1) break;
                        }
                        readBuf.flip();
                        byte[] body = new byte[readBuf.remaining()];
                        readBuf.get(body);
                        inbound.offer(new String(body, StandardCharsets.UTF_8));
                    }
                }

                Thread.sleep(100);
            }
        } catch (IOException e) {
            // Client disconnected — normal, just return.
        } finally {
            try { ch.close(); } catch (IOException ignored) {}
        }
    }

    private static void writeMessage(SocketChannel ch, byte[] body) throws IOException {
        ByteBuffer buf = ByteBuffer.allocate(HEADER_BYTES + body.length);
        buf.putInt(body.length);
        buf.put(body);
        buf.flip();
        while (buf.hasRemaining()) {
            ch.write(buf);
        }
    }
}
