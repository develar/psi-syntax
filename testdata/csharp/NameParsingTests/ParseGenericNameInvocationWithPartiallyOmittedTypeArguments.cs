class C
{
    void M<T1, T2>()
    {
        M<,int>();      // First argument omitted
        M<string,>();   // Second argument omitted
        M<,int,>();     // First and third arguments omitted (if we had 3 params)
    }
}