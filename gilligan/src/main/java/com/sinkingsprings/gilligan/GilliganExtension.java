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

    // Per-track state (updated by observer callbacks on the controller thread).
    private final String[]  names    = new String[MAX_TRACKS];
    private final String[]  types    = new String[MAX_TRACKS];
    private final boolean[] isGroup  = new boolean[MAX_TRACKS];
    private final int[]     position = new int[MAX_TRACKS];
    private final double[]  colorR   = new double[MAX_TRACKS];
    private final double[]  colorG   = new double[MAX_TRACKS];
    private final double[]  colorB   = new double[MAX_TRACKS];
    private final boolean[] exists   = new boolean[MAX_TRACKS];
    // Normalized volume (0–1) for each track — used to compute dB adjustments.
    private final double[]  volumes  = new double[MAX_TRACKS];

    private String  masterName   = "Master";
    private boolean masterExists = false;
    private double  masterVolume = 0.794; // default ≈ 0 dB

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

            track.volume().value().markInterested();
            track.volume().value().addValueObserver(v -> volumes[idx] = v);
        }

        masterTrack.exists().markInterested();
        masterTrack.exists().addValueObserver(v -> masterExists = v);
        masterTrack.name().markInterested();
        masterTrack.name().addValueObserver(v -> masterName = v);
        masterTrack.volume().value().markInterested();
        masterTrack.volume().value().addValueObserver(v -> masterVolume = v);

        ipcServer = new IpcServer(host);
        ipcServer.start();

        host.scheduleTask(this::tick, 100);

        host.showPopupNotification("Gilligan initialized");
    }

    private void tick() {
        // Execute any fix actions Skipper sent us.
        ipcServer.drainInbound(this::executeFixAction);

        // Publish the current track table.
        ipcServer.setFrame(buildTrackTable().getBytes(StandardCharsets.UTF_8));

        getHost().scheduleTask(this::tick, 100);
    }

    // ── Fix-action execution ──────────────────────────────────────────────────

    private void executeFixAction(String json) {
        if (!json.contains("\"adjust_volume\"")) return;

        double deltaDb = parseDouble(json, "delta_db");
        String trackName = parseString(json, "track_name"); // null = master

        // Bitwig automatically creates an undo entry for each setImmediately()
        // call made from a controller extension — no explicit undo block needed.
        if (trackName == null) {
            double newVol = applyDeltaDb(masterVolume, deltaDb);
            masterTrack.volume().value().setImmediately(newVol);
        } else {
            for (int i = 0; i < MAX_TRACKS; i++) {
                if (exists[i] && trackName.equals(names[i])) {
                    double newVol = applyDeltaDb(volumes[i], deltaDb);
                    trackBank.getItemAt(i).volume().value().setImmediately(newVol);
                    break;
                }
            }
        }
    }

    /**
     * Convert a dB delta to a new normalized fader value.
     *
     * Bitwig's volume fader uses approximately a square-root taper, so the
     * mapping is: volume_dB ≈ 40 * log10(normalized).  Inverting:
     *   new_normalized = old_normalized * 10^(delta_dB / 40)
     *
     * This gives the correct adjustment for a sqrt-law taper.  Small errors
     * for other taper shapes are acceptable — the undo block lets the user revert.
     */
    private static double applyDeltaDb(double normalized, double deltaDb) {
        if (normalized <= 0.0) return 0.0;
        double factor = Math.pow(10.0, deltaDb / 40.0);
        return Math.max(0.0, Math.min(1.0, normalized * factor));
    }

    // ── Track-table serialisation ─────────────────────────────────────────────

    private String buildTrackTable() {
        StringBuilder sb = new StringBuilder(4096);
        sb.append("{\"type\":\"track_table\",\"tracks\":[");
        boolean first = true;

        for (int i = 0; i < MAX_TRACKS; i++) {
            if (!exists[i]) continue;
            if (!first) sb.append(',');
            first = false;
            appendTrackRow(sb, i, names[i], types[i], isGroup[i], position[i],
                           colorR[i], colorG[i], colorB[i]);
        }

        if (masterExists) {
            if (!first) sb.append(',');
            appendTrackRow(sb, -1, masterName, "Master", false, -1,
                           0.5, 0.5, 0.5);
        }

        sb.append("]}");
        return sb.toString();
    }

    private static void appendTrackRow(StringBuilder sb, int idx,
            String name, String type, boolean group, int pos,
            double r, double g, double b) {
        sb.append("{\"idx\":").append(idx);
        sb.append(",\"name\":\"").append(escapeJson(name != null ? name : "")).append('"');
        sb.append(",\"type\":\"").append(escapeJson(type != null ? type : "")).append('"');
        sb.append(",\"is_group\":").append(group);
        sb.append(",\"position\":").append(pos);
        sb.append(",\"color_r\":").append((int) Math.round(r * 255));
        sb.append(",\"color_g\":").append((int) Math.round(g * 255));
        sb.append(",\"color_b\":").append((int) Math.round(b * 255));
        sb.append('}');
    }

    // ── Minimal JSON helpers (no external dependencies) ───────────────────────

    private static String escapeJson(String s) {
        return s.replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\n")
                .replace("\r", "\\r")
                .replace("\t", "\\t");
    }

    /** Extract a numeric value for {@code "key": <number>} from a JSON string. */
    private static double parseDouble(String json, String key) {
        String search = "\"" + key + "\":";
        int start = json.indexOf(search);
        if (start < 0) return 0.0;
        start += search.length();
        while (start < json.length() && json.charAt(start) == ' ') start++;
        int end = start;
        while (end < json.length() && ",}".indexOf(json.charAt(end)) < 0) end++;
        try {
            return Double.parseDouble(json.substring(start, end).trim());
        } catch (NumberFormatException e) {
            return 0.0;
        }
    }

    /** Extract a string value for {@code "key": "value"} from a JSON string.
     *  Returns null if the key is absent. */
    private static String parseString(String json, String key) {
        String search = "\"" + key + "\":\"";
        int start = json.indexOf(search);
        if (start < 0) return null;
        start += search.length();
        int end = start;
        while (end < json.length() && json.charAt(end) != '"') {
            if (json.charAt(end) == '\\') end++; // skip escape
            end++;
        }
        return json.substring(start, end)
                   .replace("\\\"", "\"")
                   .replace("\\\\", "\\");
    }

    @Override
    public void exit() {
        if (ipcServer != null) ipcServer.stop();
        getHost().showPopupNotification("Gilligan exited");
    }

    @Override
    public void flush() {}
}
