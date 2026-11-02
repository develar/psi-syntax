
public class Test
{
    public delegate int Delegate(in int a);

    public void Method(in int b)
    {
        void localFunc(in int c) { }

        Delegate lambda = (in int d) => d;

        Delegate anonymousDelegate = delegate (in int e) { return e; };
    }

    public int this [in int f]
    {
        get { return f; }
    }

    public static bool operator ! (in Test g)
    {
        return false;
    }
}
