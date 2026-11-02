
class C1
{
    static (T, T) Test1<T>(int a, (byte, byte) arg0)
    {
        return default((T, T));
    }

    static (T, T) Test2<T>(ref (byte, byte) arg0)
    {
        return default((T, T));
    }
}
