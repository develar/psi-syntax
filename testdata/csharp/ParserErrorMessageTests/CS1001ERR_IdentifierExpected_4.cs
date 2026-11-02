
using System;
class NamedExample
{
    static void Main(string[] args)
    {
        ExampleMethod(3, optionalint:4);
    }
    static void ExampleMethod(int required, ,int optionalint = 10)
    { }
}
