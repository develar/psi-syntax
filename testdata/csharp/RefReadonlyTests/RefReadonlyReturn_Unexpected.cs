

class Program
{
    static void Main()
    {
    }

    ref readonly int Field;

    public static ref readonly Program  operator  +(Program x, Program y)
    {
        throw null;
    }

    // this parses fine
    static async ref readonly Task M<T>()
    {
        throw null;
    }

    public ref readonly virtual int* P1 => throw null;

}
