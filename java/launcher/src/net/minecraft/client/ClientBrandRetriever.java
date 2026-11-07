package net.minecraft.client;

/** Stand-in for the vanilla class of the same name; proves our jar wins on the classpath. */
public final class ClientBrandRetriever {
    private ClientBrandRetriever() {}

    public static String getClientModName() {
        return "ferrocraft";
    }
}
