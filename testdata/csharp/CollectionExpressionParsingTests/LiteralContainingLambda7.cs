using System;
class Program
{
    static void F(Action[] a) { }
    static void Main()
    {
        F([X () => {});
    }
}