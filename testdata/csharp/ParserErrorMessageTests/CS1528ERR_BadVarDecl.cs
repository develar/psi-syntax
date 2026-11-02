
using System;
namespace nms {
public class B {
    public B(int i) {}
    public void toss () { throw new Exception("Exception thrown in function toss().");}
};
public class mine {
    private static int retval = 5;
    public static int Main()
        {
        try {B b(3);
            b.toss();
            }
        catch ( Exception e ) {retval -= 5; Console.WriteLine (e.GetMessage()); }
        return retval;
        }
    };
}
