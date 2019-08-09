public class mothra {
    public static final String NAME = "mothra-java"; 
    public static native void StartLibP2P();
    static {
        try {
            System.loadLibrary ( NAME ) ;
            
        } catch (UnsatisfiedLinkError e) {
          System.err.println("Native code library failed to load.\n" + e);
          System.exit(1);
        }
    }
}