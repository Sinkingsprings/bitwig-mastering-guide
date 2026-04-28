package com.sinkingsprings.gilligan;

import com.bitwig.extension.api.PlatformType;
import com.bitwig.extension.controller.AutoDetectionMidiPortNamesList;
import com.bitwig.extension.controller.ControllerExtensionDefinition;
import com.bitwig.extension.controller.api.ControllerHost;
import java.util.UUID;

public class GilliganExtensionDefinition extends ControllerExtensionDefinition {

    private static final UUID ID = UUID.fromString("a3f0c1d2-4e5b-6789-abcd-ef0123456789");

    @Override public String getName()            { return "Gilligan"; }
    @Override public String getAuthor()          { return "Sinkingsprings"; }
    @Override public String getVersion()         { return "0.1"; }
    @Override public UUID getId()                { return ID; }
    @Override public String getHardwareVendor()  { return "Sinkingsprings"; }
    @Override public String getHardwareModel()   { return "Mastering Guide"; }
    @Override public int getRequiredAPIVersion() { return 18; }
    @Override public int getNumMidiInPorts()     { return 0; }
    @Override public int getNumMidiOutPorts()    { return 0; }

    @Override
    public void listAutoDetectionMidiPortNames(
            AutoDetectionMidiPortNamesList list, PlatformType platformType) {
        // No MIDI ports — Gilligan is a software-only controller.
    }

    @Override
    public GilliganExtension createInstance(ControllerHost host) {
        return new GilliganExtension(this, host);
    }
}
