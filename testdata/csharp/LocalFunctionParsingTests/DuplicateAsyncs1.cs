class Program
{
    void M()
    {
        #pragma warning disable 8321
        async async void F() { }
    }
}