class C
{
    void M()
    {
        switch (new object())
        {
            bool SomeTest() => o is 42;

            case int when SomeTest():
                Console.WriteLine("answer");
                break;
        }
    }
}