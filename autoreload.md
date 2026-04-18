# Autoreload Settings

To enable autoreload, add the following code to your main file:

def run():
    # Your program here

if __name__ == '__main__':
    import sys
    if '--autoreload' in sys.argv:
        import importlib
        while True:
            try:
                importlib.reload(sys.modules['__main__'])
                run()
            except KeyboardInterrupt:
                break
    else:
        run()