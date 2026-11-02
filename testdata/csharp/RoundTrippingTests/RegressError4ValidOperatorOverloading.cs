using System;

public class A
{
    // unary
    public static bool operator true(A a)
    {
        return false;
    }

    public static bool operator false(A a)
    {
        return true;
    }

    public static A operator ++(A a)
    {
        return a;
    }

    public static A operator --(A a)
    {
        return a;
    }

    // binary
    public static bool operator <(A a, A b)
    {
        return true;
    }
    public static bool operator >(A a, A b)
    {
        return false;
    }
    public static bool operator <=(A a, A b)
    {
        return true;
    }
    public static bool operator >=(A a, A b)
    {
        return false;
    }
}