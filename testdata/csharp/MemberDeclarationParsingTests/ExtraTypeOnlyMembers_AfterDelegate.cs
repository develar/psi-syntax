namespace N
{
    delegate int D();

    // This should not be sucked into the delegate
    private void Method() { }
}