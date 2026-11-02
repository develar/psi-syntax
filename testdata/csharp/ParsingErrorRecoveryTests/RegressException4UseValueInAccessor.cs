public class MyClass
{
    public int MyProp
    {
        set { int value = 0; } // CS0136
    }
    D x;
    int this[int n]
    {
        get { return 0; }
        set { x = (value) => { value++; }; }  // CS0136
    }

    public delegate void D(int n);
    public event D MyEvent
    {
        add { object value = null; } // CS0136
        remove { }
    }
}