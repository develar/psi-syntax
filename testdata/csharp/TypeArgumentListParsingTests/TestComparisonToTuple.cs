
public class C
{
    public static void Main()
    {
        XX X = new XX();
        int a = 1, b = 2;
        bool z = X < (a, b), w = false;
    }
}

struct XX
{
    public static bool operator <(XX x, (int a, int b) arg) => true;
    public static bool operator >(XX x, (int a, int b) arg) => false;
}