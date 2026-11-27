#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>
#include <signal.h>
#include <unistd.h>

// Native WindowServer virtual-display interfaces, also exercised by Chromium:
// https://chromium.googlesource.com/chromium/src/+/HEAD/ui/display/mac/test/virtual_display_util_mac.mm
// These declarations belong only to the external test environment.
@interface CGVirtualDisplayDescriptor : NSObject
@property unsigned int vendorID;
@property unsigned int productID;
@property unsigned int serialNum;
@property unsigned int serialNumber;
@property unsigned int maxPixelsWide;
@property unsigned int maxPixelsHigh;
@property CGSize sizeInMillimeters;
@property CGPoint redPrimary;
@property CGPoint greenPrimary;
@property CGPoint bluePrimary;
@property CGPoint whitePoint;
@property(strong) NSString *name;
@property(strong) id queue;
@end
@interface CGVirtualDisplayMode : NSObject
- (instancetype)initWithWidth:(unsigned int)width height:(unsigned int)height refreshRate:(double)rate;
@end
@interface CGVirtualDisplaySettings : NSObject
@property unsigned int hiDPI;
@property(strong) NSArray *modes;
@end
@interface CGVirtualDisplay : NSObject
@property(readonly) unsigned int displayID;
- (instancetype)initWithDescriptor:(CGVirtualDisplayDescriptor *)descriptor;
- (BOOL)applySettings:(CGVirtualDisplaySettings *)settings;
@end

static volatile sig_atomic_t stopped = 0;
static void stop(int value) { stopped = value; }

int main(int argc, const char **argv) {
  @autoreleasepool {
    if (argc != 2) return 2;
    signal(SIGTERM, stop);
    signal(SIGINT, stop);
    CGVirtualDisplayDescriptor *descriptor = [CGVirtualDisplayDescriptor new];
    descriptor.name = [NSString stringWithFormat:@"Jam E2E Retina %d", getpid()];
    descriptor.vendorID = 0x4a41;
    descriptor.productID = 1;
    descriptor.serialNum = descriptor.serialNumber = (unsigned int)getpid();
    descriptor.maxPixelsWide = 2560;
    descriptor.maxPixelsHigh = 1600;
    descriptor.sizeInMillimeters = CGSizeMake(296, 185);
    descriptor.redPrimary = CGPointMake(0.64, 0.33);
    descriptor.greenPrimary = CGPointMake(0.30, 0.60);
    descriptor.bluePrimary = CGPointMake(0.15, 0.06);
    descriptor.whitePoint = CGPointMake(0.3127, 0.3290);
    descriptor.queue = dispatch_get_global_queue(QOS_CLASS_DEFAULT, 0);
    __attribute__((objc_precise_lifetime)) CGVirtualDisplay *display =
      [[CGVirtualDisplay alloc] initWithDescriptor:descriptor];
    CGVirtualDisplaySettings *settings = [CGVirtualDisplaySettings new];
    settings.hiDPI = 1;
    settings.modes = @[[[CGVirtualDisplayMode alloc] initWithWidth:1280 height:800 refreshRate:60]];
    if (!display || ![display applySettings:settings]) {
      fprintf(stderr, "WindowServer could not create the owned Retina display\n");
      return 1;
    }
    CGDirectDisplayID identifier = display.displayID;
    NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:15];
    while (!CGDisplayIsActive(identifier) && !stopped && deadline.timeIntervalSinceNow > 0) {
      [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
    }
    CGRect main = CGDisplayBounds(CGMainDisplayID());
    CGDisplayConfigRef config = NULL;
    if (!CGDisplayIsActive(identifier) || CGBeginDisplayConfiguration(&config) != kCGErrorSuccess) {
      fprintf(stderr, "The owned display did not become active/configurable\n");
      return 1;
    }
    CGError mirror = CGConfigureDisplayMirrorOfDisplay(config, identifier, kCGNullDirectDisplay);
    CGError origin = CGConfigureDisplayOrigin(config, identifier, (int)CGRectGetMaxX(main), (int)main.origin.y);
    if (mirror != kCGErrorSuccess || origin != kCGErrorSuccess) {
      fprintf(stderr, "Owned display configuration failed: mirror=%d origin=%d\n", mirror, origin);
      CGCancelDisplayConfiguration(config);
      return 1;
    }
    CGError committed = CGCompleteDisplayConfiguration(config, kCGConfigureForSession);
    if (committed != kCGErrorSuccess) {
      fprintf(stderr, "Owned display configuration commit failed: %d\n", committed);
      return 1;
    }
    CGDisplayModeRef mode = CGDisplayCopyDisplayMode(identifier);
    if (!mode || CGDisplayModeGetPixelWidth(mode) != 2 * CGDisplayModeGetWidth(mode)) {
      fprintf(stderr, "The owned output is not Retina: pixels=%zu points=%zu\n",
              mode ? CGDisplayModeGetPixelWidth(mode) : 0,
              mode ? CGDisplayModeGetWidth(mode) : 0);
      if (mode) CGDisplayModeRelease(mode);
      return 1;
    }
    CGRect rect = CGDisplayBounds(identifier);
    NSDictionary *ready = @{
      @"pid": @(getpid()), @"display": @(identifier), @"scale": @2,
      @"rect": @{@"x": @(rect.origin.x), @"y": @(rect.origin.y),
                 @"width": @(rect.size.width), @"height": @(rect.size.height)},
      @"pixel_width": @(CGDisplayModeGetPixelWidth(mode)),
      @"logical_width": @(CGDisplayModeGetWidth(mode))
    };
    CGDisplayModeRelease(mode);
    NSData *json = [NSJSONSerialization dataWithJSONObject:ready options:0 error:NULL];
    if (![json writeToFile:[NSString stringWithUTF8String:argv[1]] options:NSDataWritingAtomic error:NULL]) return 1;
    deadline = [NSDate dateWithTimeIntervalSinceNow:240];
    while (!stopped && deadline.timeIntervalSinceNow > 0) {
      [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
    }
    display = nil;
    // The parent verifies disappearance through WindowServer after this
    // process exits and all its CoreGraphics/autorelease references are gone.
    return stopped ? 0 : 1;
  }
}
