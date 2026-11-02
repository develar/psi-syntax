class C
{
    void M()
    {
        [() => {}][rand.Next()](A).B();
    }
}