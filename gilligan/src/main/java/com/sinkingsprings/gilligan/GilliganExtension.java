package com.sinkingsprings.gilligan;

import com.bitwig.extension.controller.ControllerExtension;
import com.bitwig.extension.controller.api.ControllerHost;
import com.bitwig.extension.controller.api.MasterTrack;
import com.bitwig.extension.controller.api.Track;
import com.bitwig.extension.controller.api.TrackBank;
import com.sinkingsprings.gilligan.ipc.IpcServer;
import java.nio.charset.StandardCharsets;

public class GilliganExtension extends ControllerExtension {

    private static final int MAX_TRACKS = 64;

    private TrackBank trackBank;
    private MasterTrack masterTrack;
    private IpcServer ipcServer;

    // Per-track state captured by observer callbacks (controller thread).
    private final String[] names     = new String[MAX_TRACKS];
    private final String[] types     = new String[MAX_TRACKS];
    private final boolean[] isGroup  = new boolean[MAX_TRACKS];
    private final int[]    position  = new int[MAX_TRACKS];
    private final double[] colorR    = new double[MAX_TRACKS];
    private final double[] colorG    = new double[MAX_TRACKS];
    private final double[] colorB    = new double[MAX_TRACKS];
    private final double[] vuL       = new double[MAX_TRACKS];
    private final double[] vuR       = new double[MAX_TRACKS];
    private final boolean[] exists   = new boolean[MAX_TRACKS];

    private String masterName = "Master";
    private double masterVuL  = 0.0;
    private double masterVuR  = 0.0;
    private boolean masterExists = false;

    protected GilliganExtension(GilliganExtensionDefinition definition, ControllerHost host) {
        super(definition, host);
    }

    @Override
    public void init() {
        ControllerHost host = getHost();

        // Flat TrackBank: 64 tracks, 0 sends, 0 scenes, hasFlatTrackList=true.
        trackBank = host.createTrackBank(MAX_TRACKS, 0, 0, true);
        masterTrack = host.createMasterTrack(0);

        for (int i = 0; i < MAX_TRACKS; i++) {
            final int idx = i;
            Track track = trackBank.getItemAt(i);

            track.exists().markInterested();
            track.exists().addValueObserver(v -> exists[idx] = v);

            track.name().markInterested();
            track.name().addValueObserver(v -> names[idx] = v);

            track.trackType().markInterested();
            track.trackType().addValueObserver(v -> types[idx] = v);

            track.isGroup().markInterested();
            track.isGroup().addValueObserver(v -> isGroup[idx] = v);

            track.position().markInterested();
            track.position().addValueObserver(v -> position[idx] = v);

            track.color().markInterested();
            track.color().addValueObserver((r, g, b) -> {
                colorR[idx] = r;
                colorG[idx] = g;
                colorB[idx] = b;
            });

            // VU: scale 0–127, channel 0=L 1=R, peakMode=true
            track.addVuMeterObserver(127, 0, true, v -> vuL[idx] = v / 127.0);
            track.addVuMeterObserver(127, 1, true, v -> vuR[idx] = v / 127.0);
        }

        masterTrack.exists().markInterested();
        masterTrack.exists().addValueObserver(v -> masterExists = v);
        masterTrack.name().markInterested();
        masterTrack.name().addValueObserver(v -> masterName = v);
        masterTrack.addVuMeterObserver(127, 0, true, v -> masterVuL = v / 127.0);
        masterTrack.addVuMeterObserver(127, 1, true, v -> masterVuR = v / 127.0);

        ipcServer = new IpcServer(host);
        ipcServer.start();

        // Kick off the 100 ms publish loop.
        host.scheduleTask(this::tick, 100);

        host.showPopupNotification("Gilligan initialized");
    }

    private void tick() {
        // Drain any inbound messages from Skipper (FixActions arrive here in Phase 12).
        ipcServer.drainInbound(msg -> getHost().println("Gilligan inbound: " + msg));

        // Serialise the current track table and hand it to the IPC worker.
        ipcServer.setFrame(buildTrackTable().getBytes(StandardCharsets.UTF_8));

        getHost().scheduleTask(this::tick, 100);
    }

    private String buildTrackTable() {
        StringBuilder sb = new StringBuilder(4096);
        sb.append("{\"type\":\"track_table\",\"tracks\":[");
        boolean first = true;

        for (int i = 0; i < MAX_TRACKS; i++) {
            if (!exists[i]) continue;
            if (!first) sb.append(',');
            first = false;
            appendTrack(sb, i, names[i], types[i], isGroup[i], position[i],
                        colorR[i], colorG[i], colorB[i], vuL[i], vuR[i]);
        }

        if (masterExists) {
            if (!first) sb.append(',');
            appendTrackRaw(sb, -1, masterName, "Master", false, -1,
                           0.5, 0.5, 0.5, masterVuL, masterVuR);
        }

        sb.append("]}");
        return sb.toString();
    }

    private static void appendTrack(StringBuilder sb, int idx,
            String name, String type, boolean group, int pos,
            double r, double g, double b, double vl, double vr) {
        appendTrackRaw(sb, idx, name, type, group, pos, r, g, b, vl, vr);
    }

    private static void appendTrackRaw(StringBuilder sb, int idx,
            String name, String type, boolean group, int pos,
            double r, double g, double b, double vl, double vr) {
        sb.append("{\"idx\":").append(idx);
        sb.append(",\"name\":\"").append(escapeJson(name != null ? name : "")).append('"');
        sb.append(",\"type\":\"").append(escapeJson(type != null ? type : "")).append('"');
        sb.append(",\"is_group\":").append(group);
        sb.append(",\"position\":").append(pos);
        sb.append(",\"color_r\":").append((int) Math.round(r * 255));
        sb.append(",\"color_g\":").append((int) Math.round(g * 255));
        sb.append(",\"color_b\":").append((int) Math.round(b * 255));
        sb.append(",\"vu_l\":").append(String.format("%.3f", vl));
        sb.append(",\"vu_r\":").append(String.format("%.3f", vr));
        sb.append('}');
    }

    private static String escapeJson(String s) {
        return s.replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\n")
                .replace("\r", "\\r")
                .replace("\t", "\\t");
    }

    @Override
    public void exit() {
        if (ipcServer != null) ipcServer.stop();
        getHost().showPopupNotification("Gilligan exited");
    }

    @Override
    public void flush() {}
}
