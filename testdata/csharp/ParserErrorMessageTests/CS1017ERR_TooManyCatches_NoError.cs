
using System;
namespace nms {
public class S : Exception {
};
public class S1 : Exception {
};
public class mine {
    private static int retval = 2;
    public static int Main()
        {
        try {
                throw new S();
        }
        catch when (true) {}
        catch (S1) {}
        catch (S) {}
        if (retval == 0) Console.WriteLine("PASS");
        else Console.WriteLine("FAIL");
           return retval;
        }
    };
}
